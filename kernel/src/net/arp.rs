use alloc::collections::BTreeMap;
use spin::RwLock;
use super::ethernet::EthernetFrame;
use super::ip::IPV4Address;
use super::packet::PacketHeader;

#[repr(C, packed)]
pub struct ARP {
    hardware_type: u16,
    protocol_type: u16,
    hardware_addr_length: u8,
    protocol_addr_length: u8,
    opcode: u16,
    source_hardware_addr: [u8; 6],
    source_protocol_addr: [u8; 4],
    dest_hardware_addr: [u8; 6],
    dest_protocol_addr: [u8; 4],
}

impl ARP {
    pub fn request(src_mac: [u8; 6], src_ip: [u8; 4], lookup: [u8; 4]) -> Self {
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

    pub fn response(src_mac: [u8; 6], src_ip: [u8; 4], dest_mac: [u8; 6], dest_ip: [u8; 4]) -> Self {
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

    pub fn announce(mac: [u8; 6], ip: [u8; 4]) -> Self {
        Self::request(mac, ip, ip)
    }

    /// Respond to an ARP request packet with the system MAC and IP
    pub fn respond(&self, mac: [u8; 6], ip: [u8; 4]) -> Option<Self> {
        if self.opcode != 1u16.to_be() {
            return None;
        }
        let response = Self::response(mac, ip, self.source_hardware_addr, self.source_protocol_addr);
        Some(response)
    }
}

impl PacketHeader for ARP {}

pub static TRANSLATIONS: RwLock<BTreeMap<[u8; 4], [u8; 6]>> = RwLock::new(BTreeMap::new());

