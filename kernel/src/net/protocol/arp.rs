//! Address Resolution Protocol (ARP) is the network protocol used to discover
//! the hardware address (MAC, etc) of devices on the network.
//! ARP Packets are sent at the data link layer, and are independent of the
//! networking protocol being used.
//! The underlying protocol allows sending probe requests, where a single host
//! looks for a specific device. It also supports broadcast requests, where a
//! device can tell all interested parties that it is available at a specific
//! location.

use crate::net::hardware::HardwareAddress;

use super::{ipv4::Ipv4Address, packet::PacketHeader};

/// The ARP Packet contains just enough data for the hardware and protocol
/// address for both the source and destination devices.
#[repr(C, packed)]
pub struct ArpPacket {
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
    pub source_protocol_addr: Ipv4Address,
    /// 6-octet buffer for the destination hardware address
    pub dest_hardware_addr: HardwareAddress,
    /// 4-octet buffer for the destination protocol address
    pub dest_protocol_addr: Ipv4Address,
}

impl ArpPacket {
    /// Construct an ARP request packet, used for searching for a specific device
    pub fn request(src_mac: HardwareAddress, src_ip: Ipv4Address, lookup: Ipv4Address) -> Self {
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
        src_ip: Ipv4Address,
        dest_mac: HardwareAddress,
        dest_ip: Ipv4Address,
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
    pub fn announce(mac: HardwareAddress, ip: Ipv4Address) -> Self {
        Self::request(mac, ip, ip)
    }

    /// Construct a response to a specific incoming ARP request
    pub fn respond_to(request: &Self, mac: HardwareAddress, ip: Ipv4Address) -> Option<Self> {
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

    pub fn is_request(&self) -> bool {
        self.opcode == 1u16.to_be()
    }
}

impl PacketHeader for ArpPacket {}
