//! Address Resolution Protocol (ARP) is the network protocol used to discover
//! the hardware address (MAC, etc) of devices on the network.
//! ARP Packets are sent at the data link layer, and are independent of the
//! networking protocol being used.
//! The underlying protocol allows sending probe requests, where a single host
//! looks for a specific device. It also supports broadcast requests, where a
//! device can tell all interested parties that it is available at a specific
//! location.

use crate::task::actions::handle::{
    create_file_handle, handle_op_close, handle_op_open, handle_op_write,
};
use crate::task::actions::lifecycle::wait_for_io;
use crate::task::id::TaskID;
use crate::task::switching::{get_current_id, get_task};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use spin::RwLock;

use super::error::NetError;
use super::ethernet::{EthernetFrameHeader, HardwareAddress};
use super::ip::IPV4Address;
use super::packet::PacketHeader;
use super::with_active_device;

/// The ARP Packet contains just enough data for the hardware and protocol
/// address for both the source and destination devices.
#[repr(C, packed)]
pub struct ARPPacket {
    /// Network link type; 1 for Ethernet
    pub hardware_type: u16,
    /// Network protocol type, using the same values as EtherType; 0x0800 for IPv4
    pub protocol_type: u16,
    /// Length of hardware address in octets; 6 for Ethernet
    pub hardware_addr_length: u8,
    /// Length of protocol address in octets; 4 for IPv4
    pub protocol_addr_length: u8,
    /// ARP operation; 1 for request, 2 for response
    pub opcode: u16,

    // The sizes of the following fields are declared earlier by the `_length`
    // properties. Since we only support Ethernet and IPv4, we can hard-code
    // these to 6 and 4 octets respectively.
    /// 6-octet buffer for the source hardware address
    pub source_hardware_addr: HardwareAddress,
    /// 4-octet buffer for the source protocol address
    pub source_protocol_addr: IPV4Address,

    pub dest_hardware_addr: HardwareAddress,
    pub dest_protocol_addr: IPV4Address,
}

impl ARPPacket {
    /// Construct an ARP request packet, used for searching for a specific device
    pub fn request(src_mac: HardwareAddress, src_ip: IPV4Address, lookup: IPV4Address) -> Self {
        Self {
            hardware_type: 1u16.to_be(),
            protocol_type: 0x0800u16.to_be(),
            hardware_addr_length: 6,
            protocol_addr_length: 4,
            opcode: 1u16.to_be(),
            source_hardware_addr: src_mac,
            source_protocol_addr: src_ip,
            dest_hardware_addr: HardwareAddress([0; 6]),
            dest_protocol_addr: lookup,
        }
    }

    /// Construct an ARP response packet, used for responding to a request
    pub fn response(
        src_mac: HardwareAddress,
        src_ip: IPV4Address,
        dest_mac: HardwareAddress,
        dest_ip: IPV4Address,
    ) -> Self {
        Self {
            hardware_type: 1u16.to_be(),
            protocol_type: 0x0800u16.to_be(),
            hardware_addr_length: 6,
            protocol_addr_length: 4,
            opcode: 2u16.to_be(),
            source_hardware_addr: src_mac,
            source_protocol_addr: src_ip,
            dest_hardware_addr: dest_mac,
            dest_protocol_addr: dest_ip,
        }
    }

    /// Construct an announcement packet, used for telling all networked devices
    /// about this device's MAC and IP
    pub fn announce(mac: HardwareAddress, ip: IPV4Address) -> Self {
        Self::request(mac, ip, ip)
    }

    /// Construct a response to a specific incoming ARP request
    pub fn respond_to(request: &Self, mac: HardwareAddress, ip: IPV4Address) -> Option<Self> {
        if request.opcode != 1u16.to_be() {
            return None;
        }
        let response = Self::response(
            mac,
            ip,
            request.source_hardware_addr,
            request.source_protocol_addr,
        );
        Some(response)
    }
}

impl PacketHeader for ARPPacket {}

pub struct ARPTable {
    table: BTreeMap<IPV4Address, HardwareAddress>,
    pending_lookups: BTreeSet<IPV4Address>,
}

impl ARPTable {
    pub fn new() -> Self {
        Self {
            table: BTreeMap::new(),
            pending_lookups: BTreeSet::new(),
        }
    }

    pub fn add_entry(&mut self, ip: IPV4Address, mac: HardwareAddress) {
        self.table.insert(ip, mac);
    }

    /// Add a new record to the table. Returns true if a search was pending for
    /// this IP, otherwise returns false.
    pub fn resolve_lookup(&mut self, ip: IPV4Address, mac: HardwareAddress) -> bool {
        let was_pending = self.pending_lookups.remove(&ip);
        self.add_entry(ip, mac);
        was_pending
    }

    /// Get the hardware address for a given IP address.
    /// If it is known, it returns ARPLookupResult::Resolved
    /// If another task has asked for this IP, it returns ARPLookupResult::Pending
    /// If it is not known, and there is no pending search, it returns ARPLookupResult::Unknown
    pub fn lookup(&self, ip: IPV4Address) -> ARPLookupResult {
        if self.pending_lookups.contains(&ip) {
            return ARPLookupResult::Pending;
        }
        match self.table.get(&ip) {
            Some(mac) => ARPLookupResult::Resolved(*mac),
            None => ARPLookupResult::Unknown,
        }
    }
}

pub enum ARPLookupResult {
    Unknown,
    Pending,
    Resolved(HardwareAddress),
}

static TRANSLATIONS: RwLock<BTreeMap<IPV4Address, HardwareAddress>> = RwLock::new(BTreeMap::new());

static PENDING_SEARCHES: RwLock<BTreeMap<IPV4Address, Vec<TaskID>>> = RwLock::new(BTreeMap::new());

pub fn add_network_translation(protocol_addr: IPV4Address, hardware_addr: HardwareAddress) {
    let mut translations = TRANSLATIONS.write();
    let prev = translations.insert(protocol_addr, hardware_addr);
    match prev {
        Some(previous_mapping) => {
            if previous_mapping != hardware_addr {
                // idk, do something?
            }
        }
        None => (),
    }
}

pub fn send_arp_request(lookup_ip: IPV4Address) -> Result<(), NetError> {
    let (device_mac, device_name, device_ip) =
        with_active_device(|netdev| (netdev.mac, netdev.device_name.clone(), *netdev.ip.read()))
            .map_err(|_| NetError::NoNetDevice)?;

    let local_ip = device_ip.expect("Needs IP addr");
    let arp = ARPPacket::request(device_mac, local_ip, lookup_ip);
    let mut total_frame =
        Vec::with_capacity(EthernetFrameHeader::get_size() + ARPPacket::get_size());
    let eth_header = EthernetFrameHeader::broadcast_arp(device_mac);
    total_frame.extend_from_slice(eth_header.as_u8_buffer());
    total_frame.extend_from_slice(arp.as_u8_buffer());

    let dev = create_file_handle();
    handle_op_open(dev, &device_name)
        .wait_for_result()
        .map_err(|_| NetError::DeviceDriverError)?;
    handle_op_write(dev, &total_frame)
        .wait_for_result()
        .map_err(|_| NetError::DeviceDriverError)?;
    handle_op_close(dev)
        .wait_for_result()
        .map_err(|_| NetError::DeviceDriverError)?;
    Ok(())
}

pub fn handle_arp_announcement(payload: &[u8]) {
    if payload.len() < ARPPacket::get_size() {
        crate::kprintln!("Invalid ARP packet");
        return;
    }
    let arp = unsafe { &*(payload.as_ptr() as *const ARPPacket) };
    add_network_translation(arp.source_protocol_addr, arp.source_hardware_addr);

    let pending = PENDING_SEARCHES.write().remove(&arp.source_protocol_addr);
    if let Some(blocked_list) = pending {
        for id in blocked_list {
            match get_task(id) {
                Some(lock) => lock.write().io_complete(),
                None => (),
            }
        }
    }
}

pub fn resolve_mac_from_ip(ip: IPV4Address) -> Result<HardwareAddress, NetError> {
    match TRANSLATIONS.read().get(&ip) {
        Some(cached) => return Ok(*cached),
        None => (),
    }

    // if the value is not known, start a search
    let current_id = get_current_id();
    {
        let mut pending = PENDING_SEARCHES.write();
        if let Some(blocked) = pending.get_mut(&ip) {
            blocked.push(current_id);
        } else {
            let mut blocked = Vec::new();
            blocked.push(current_id);
            pending.insert(ip, blocked);
        }
    }
    send_arp_request(ip)?;

    if let Some(cached) = TRANSLATIONS.read().get(&ip) {
        return Ok(*cached);
    }
    wait_for_io(Some(5000));

    match TRANSLATIONS.read().get(&ip) {
        Some(cached) => Ok(*cached),
        None => Err(NetError::AddressNotResolved),
    }
}
