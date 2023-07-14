use super::packet::PacketHeader;

pub const ETHERTYPE_IP: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;

#[repr(C, packed)]
pub struct EthernetFrame {
    pub dest_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ethertype: u16,
}

impl EthernetFrame {
    pub fn new(src: [u8; 6], dest: [u8; 6], ethertype: u16) -> Self {
        Self {
            src_mac: src,
            dest_mac: dest,
            ethertype: ethertype.to_be(),
        }
    }

    pub fn get_ethertype(&self) -> u16 {
        self.ethertype.to_be()
    }

    pub fn broadcast_arp(src: [u8; 6]) -> Self {
        Self::new(src, [0xff; 6], ETHERTYPE_ARP)
    }

    pub fn new_ipv4(src: [u8; 6], dest: [u8; 6]) -> Self {
        Self::new(src, dest, ETHERTYPE_IP)
    }
}

impl PacketHeader for EthernetFrame {}

