use super::packet::PacketHeader;

/// Wrapper type for a 6-octet hardware MAC address.
/// This should be passed between methods of the network stack, rather than
/// a raw 6-byte array.
#[derive(Copy, Clone, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct HardwareAddress(pub [u8; 6]);

impl HardwareAddress {
    /// shorthand for a MAC address that broadcasts to all devices (all octets
    /// set to 0xff)
    pub fn broadcast() -> Self {
        Self([0xff; 6])
    }
}

impl core::ops::Deref for HardwareAddress {
    type Target = [u8; 6];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for HardwareAddress {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl core::fmt::Display for HardwareAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(core::format_args!(
            "{}:{}:{}:{}:{}:{}",
            self[0],
            self[1],
            self[2],
            self[3],
            self[4],
            self[5]
        ))
    }
}

pub const ETHERTYPE_IP: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;

#[repr(C, packed)]
pub struct EthernetFrameHeader {
    pub dest_mac: HardwareAddress,
    pub src_mac: HardwareAddress,
    pub ethertype: u16,
}

impl EthernetFrameHeader {
    /// Create a new ethernet frame with the given source, destination, and type
    pub fn new(src: HardwareAddress, dest: HardwareAddress, ethertype: u16) -> Self {
        Self {
            src_mac: src,
            dest_mac: dest,
            ethertype: ethertype.to_be(),
        }
    }

    pub fn get_ethertype(&self) -> u16 {
        self.ethertype.to_be()
    }

    /// Create an ARP broadcast packet from a given source MAC address
    pub fn broadcast_arp(src: HardwareAddress) -> Self {
        Self::new(src, HardwareAddress::broadcast(), ETHERTYPE_ARP)
    }

    /// Create an IPv4 packet with a given source and destination MAC
    pub fn new_ipv4(src: HardwareAddress, dest: HardwareAddress) -> Self {
        Self::new(src, dest, ETHERTYPE_IP)
    }
}

impl PacketHeader for EthernetFrameHeader {}
