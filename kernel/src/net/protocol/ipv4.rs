use super::{checksum::Checksum, packet::PacketHeader};

/// Transparent wrapper for an IPV4 address, used for type safety
#[derive(Copy, Clone, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Ipv4Address(pub [u8; 4]);

impl core::ops::Deref for Ipv4Address {
    type Target = [u8; 4];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for Ipv4Address {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for Ipv4Address {
    fn default() -> Self {
        Self([0; 4])
    }
}

impl core::fmt::Display for Ipv4Address {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(core::format_args!(
            "{}.{}.{}.{}",
            self[0],
            self[1],
            self[2],
            self[3]
        ))
    }
}

impl core::ops::BitAnd for Ipv4Address {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        let mut result = self;
        for i in 0..4 {
            result[i] &= rhs[i];
        }
        result
    }
}

/// Enum for supported IP protocol types
#[repr(u8)]
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum IpProtocolType {
    Icmp = 0x01,
    Tcp = 0x06,
    Udp = 0x11,
}

/// Header for sending and receiving IPV4 packets
#[repr(C, packed)]
pub struct Ipv4Header {
    /// 4-bit version, 4-bit header length
    pub version_header_len: u8,
    /// differentiated services and congested network notification
    pub diff_services: u8,
    /// total length of the packet, including header and data
    pub total_length: u16,
    /// unique identifier for the packet, can be used for grouping fragments
    /// into a single packet
    pub identification: u16,
    /// fragmentation information. The highest 3 bits are flags marking if
    /// a packet can or has been fragmented. The remaining 13 bits are the
    /// packet's offset in the original packet, used for reassembling
    pub fragment: u16,
    /// time to live, decremented by each router that processes the packet
    pub ttl: u8,
    /// protocol of the data layer iwthin the ipv4 packet
    pub protocol: IpProtocolType,
    /// checksum of the header, computed using [`crate::net::checksum::Checksum`]
    pub checksum: u16,
    /// IP address of the sender
    pub source: Ipv4Address,
    /// IP address of the destination, used for routing to the final node
    pub dest: Ipv4Address,
}

impl Ipv4Header {
    pub fn new(
        source: Ipv4Address,
        dest: Ipv4Address,
        content_len: u16,
        ttl: u8,
        protocol: IpProtocolType,
    ) -> Self {
        let mut header = Self {
            version_header_len: 0x45, // Version 4, header length 5 (20 bytes)
            diff_services: 0,
            total_length: (20 + content_len).to_be(), // 20 bytes for the header + content length
            identification: 0,
            fragment: 0,
            ttl,
            protocol,
            checksum: 0,
            source,
            dest,
        };
        // Once the header is constructed, we can compute the checksum
        header.checksum = header.compute_checksum();
        header
    }

    /// Shorthand for new() using the UDP protocol
    pub fn new_udp(source: Ipv4Address, dest: Ipv4Address, content_len: u16, ttl: u8) -> Self {
        Self::new(source, dest, content_len, ttl, IpProtocolType::Udp)
    }

    /// Shorthand for new() using the TCP protocol
    pub fn new_tcp(source: Ipv4Address, dest: Ipv4Address, content_len: u16, ttl: u8) -> Self {
        Self::new(source, dest, content_len, ttl, IpProtocolType::Tcp)
    }

    /// Computes the checksum for the header
    pub fn compute_checksum(&self) -> u16 {
        // cast header as 10 16-bit numbers
        let slice = <Self as PacketHeader>::try_as_u16_buffer(self).unwrap();

        let mut checksum = Checksum::new();
        for &word in slice {
            checksum.add_u16(word);
        }
        checksum.compute()
    }
}

impl PacketHeader for Ipv4Header {}
