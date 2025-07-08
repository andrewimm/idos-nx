use super::super::hardware::HardwareAddress;
use super::packet::PacketHeader;

/// Header for a raw ethernet frame, the lowest layer of the network stack that
/// the OS deals with.
#[repr(C, packed)]
pub struct EthernetFrameHeader {
    pub dest_mac: HardwareAddress,
    pub src_mac: HardwareAddress,
    pub ethertype: u16,
}

impl EthernetFrameHeader {
    pub const ETHERTYPE_IP: u16 = 0x0800;
    pub const ETHERTYPE_ARP: u16 = 0x0806;

    /// Create a new ethernet frame with the given source, destination, and type
    pub fn new(src: HardwareAddress, dest: HardwareAddress, ethertype: u16) -> Self {
        Self {
            src_mac: src,
            dest_mac: dest,
            ethertype: ethertype.to_be(),
        }
    }

    pub fn get_ethertype(&self) -> u16 {
        u16::from_be(self.ethertype)
    }

    /// Create an ARP broadcast packet from a given source MAC address
    pub fn broadcast_arp(src: HardwareAddress) -> Self {
        Self::new(src, HardwareAddress::broadcast(), Self::ETHERTYPE_ARP)
    }

    /// Create an IPv4 packet with a given source and destination MAC
    pub fn new_ipv4(src: HardwareAddress, dest: HardwareAddress) -> Self {
        Self::new(src, dest, Self::ETHERTYPE_IP)
    }
}

impl PacketHeader for EthernetFrameHeader {}
