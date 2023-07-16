use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::RwLock;
use crate::task::actions::io::{open_path, write_file, close_file};
use crate::task::actions::lifecycle::wait_for_io;
use crate::task::id::TaskID;
use crate::task::switching::{get_current_id, get_task};

use super::error::NetError;
use super::ethernet::EthernetFrame;
use super::ip::IPV4Address;
use super::packet::PacketHeader;
use super::with_active_device;

#[repr(C, packed)]
pub struct ARP {
    hardware_type: u16,
    protocol_type: u16,
    hardware_addr_length: u8,
    protocol_addr_length: u8,
    opcode: u16,
    source_hardware_addr: [u8; 6],
    source_protocol_addr: IPV4Address,
    dest_hardware_addr: [u8; 6],
    dest_protocol_addr: IPV4Address,
}

impl ARP {
    pub fn request(src_mac: [u8; 6], src_ip: IPV4Address, lookup: IPV4Address) -> Self {
        Self {
            hardware_type: 1u16.to_be(),
            protocol_type: 0x0800u16.to_be(),
            hardware_addr_length: 6,
            protocol_addr_length: 4,
            opcode: 1u16.to_be(),
            source_hardware_addr: src_mac,
            source_protocol_addr: src_ip,
            dest_hardware_addr: [0; 6],
            dest_protocol_addr: lookup,
        }
    }

    pub fn response(src_mac: [u8; 6], src_ip: IPV4Address, dest_mac: [u8; 6], dest_ip: IPV4Address) -> Self {
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

    pub fn announce(mac: [u8; 6], ip: IPV4Address) -> Self {
        Self::request(mac, ip, ip)
    }

    /// Respond to an ARP request packet with the system MAC and IP
    pub fn respond(&self, mac: [u8; 6], ip: IPV4Address) -> Option<Self> {
        if self.opcode != 1u16.to_be() {
            return None;
        }
        let response = Self::response(mac, ip, self.source_hardware_addr, self.source_protocol_addr);
        Some(response)
    }
}

impl PacketHeader for ARP {}

static TRANSLATIONS: RwLock<BTreeMap<IPV4Address, [u8; 6]>> = RwLock::new(BTreeMap::new());

static PENDING_SEARCHES: RwLock<BTreeMap<IPV4Address, Vec<TaskID>>> = RwLock::new(BTreeMap::new());

pub fn send_arp_request(lookup_ip: IPV4Address) -> Result<(), NetError> {
    let (device_mac, device_name, device_ip) = with_active_device(|netdev| (netdev.mac, netdev.device_name.clone(), *netdev.ip.read()))
        .map_err(|_| NetError::NoNetDevice)?;

    let local_ip = device_ip.expect("Can't send an ARP until I have an IP myself");
    let arp = ARP::request(device_mac, local_ip, lookup_ip);
    let mut total_frame = Vec::with_capacity(EthernetFrame::get_size() + ARP::get_size());
    let eth_header = EthernetFrame::broadcast_arp(device_mac);
    total_frame.extend_from_slice(eth_header.as_buffer());
    total_frame.extend_from_slice(arp.as_buffer());

    let dev = open_path(&device_name).map_err(|_| NetError::DeviceDriverError)?;
    write_file(dev, &total_frame).map_err(|_| NetError::DeviceDriverError)?;
    close_file(dev).map_err(|_| NetError::DeviceDriverError)?;
    Ok(())
}

pub fn handle_arp_announcement(payload: &[u8]) {
    if payload.len() < ARP::get_size() {
        crate::kprintln!("Invalid ARP packet");
        return;
    }
    let arp = unsafe { &*(payload.as_ptr() as *const ARP) };

    crate::kprintln!(
        "ARP announcement: {} is at {}:{}:{}:{}:{}:{}",
        arp.source_protocol_addr,
        arp.source_hardware_addr[0],
        arp.source_hardware_addr[1],
        arp.source_hardware_addr[2],
        arp.source_hardware_addr[3],
        arp.source_hardware_addr[4],
        arp.source_hardware_addr[5],
    );
    TRANSLATIONS.write().insert(arp.source_protocol_addr, arp.source_hardware_addr);

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

pub fn resolve_mac_from_ip(ip: IPV4Address) -> Result<[u8; 6], NetError> {
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
    send_arp_request(ip);

    if let Some(cached) = TRANSLATIONS.read().get(&ip) {
        return Ok(*cached);
    }
    wait_for_io(Some(5000));

    match TRANSLATIONS.read().get(&ip) {
        Some(cached) => Ok(*cached),
        None => Err(NetError::AddressNotResolved),
    }
}

