use super::{checksum::Checksum, packet::PacketHeader};

/// Transparent wrapper for an IPV4 address, used for type safety
#[derive(Copy, Clone, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct IPV4Address(pub [u8; 4]);

impl core::ops::Deref for IPV4Address {
    type Target = [u8; 4];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for IPV4Address {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for IPV4Address {
    fn default() -> Self {
        Self([0; 4])
    }
}

impl core::fmt::Display for IPV4Address {
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

/// Enum for supported IP protocol types
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum IPProtocolType {
    TCP = 0x06,
    UDP = 0x11,
}

/// Header for sending and recieving IPV4 packets
#[repr(C, packed)]
pub struct IPV4Header {
    // 4-bit version, 4-bit header length
    pub version_header_len: u8,
    // differentiated services and congested network notif
    pub diff_services: u8,
    // total length of the packet, including header and data
    pub total_length: u16,
    // unique identifier for the packet, can be used for grouping fragments
    // into a single packet
    pub identification: u16,
    // fragmentation information. The highest 3 bits are flags marking if
    // a packet can or has been fragmented. The remaining 13 bits are the
    // packet's offset relative to the beginning of the original packet
    pub fragment: u16,
    // time to live, decremented by each router that forwards the packet
    pub ttl: u8,
    // protocol of the data layer within ths IPV4 packet
    pub protocol: IPProtocolType,
    // checksum of the header, computed using [`super::checksum::Checksum`]
    pub checksum: u16,
    // IP address of the original sender
    pub source: IPV4Address,
    // IP address for the destination, used for routing to the final node
    pub dest: IPV4Address,
}

impl IPV4Header {
    pub fn new(
        source: IPV4Address,
        dest: IPV4Address,
        content_len: u16,
        ttl: u8,
        protocol_type: IPProtocolType,
    ) -> Self {
        let mut header = Self {
            // set version to 4, header length to 5 32-bit words
            version_header_len: 0x45,
            diff_services: 0,
            // account for the 20 bytes of this header
            total_length: (20 + content_len).to_be(),
            identification: 0,
            fragment: 0,
            ttl,
            protocol: protocol_type,
            checksum: 0,
            source,
            dest,
        };
        // Once the header has been constructed, we can compute the actual checksum
        header.checksum = header.compute_checksum();

        header
    }

    /// shorthand for new() using the UDP protocol
    pub fn new_udp(source: IPV4Address, dest: IPV4Address, content_len: u16, ttl: u8) -> Self {
        Self::new(source, dest, content_len, ttl, IPProtocolType::UDP)
    }

    /// shorthand for new() using the TCP protocol
    pub fn new_tcp(source: IPV4Address, dest: IPV4Address, content_len: u16, ttl: u8) -> Self {
        Self::new(source, dest, content_len, ttl, IPProtocolType::TCP)
    }

    pub fn compute_checksum(&self) -> u16 {
        // cast header as 10 16-bit numbers
        let slice = <Self as PacketHeader>::try_as_u16_buffer(self).unwrap();

        let mut checksum = Checksum::new();
        for i in 0..slice.len() {
            checksum.add_u16(slice[i]);
        }
        checksum.compute()
    }
}

impl PacketHeader for IPV4Header {}
