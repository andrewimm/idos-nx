use super::{checksum::Checksum, packet::PacketHeader};

/// Transparent wrapper for an IPV4 address, used for type safety
#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Ipv4Address(pub [u8; 4]);

impl Ipv4Address {
    pub fn parse(addr: &str) -> Option<Self> {
        let bytes = addr.as_bytes();

        if bytes.len() < 7 || bytes.len() > 15 {
            // can't be less than 7 (eg 0.0.0.0) or more than 15
            return None;
        }

        let mut index = 0;
        let mut octets = [0u8; 4];
        for i in 0..4 {
            octets[i] = parse_octet(&bytes[index..])?;
            index += 1;
            if octets[i] > 9 {
                index += 1;
            }
            if octets[i] > 99 {
                index += 1;
            }

            if i < 3 {
                if index >= bytes.len() - 1 || bytes[index] != b'.' {
                    return None; // Missing separator
                }
                index += 1;
            }
        }
        if index != bytes.len() {
            return None; // Extra characters
        }

        Some(Self(octets))
    }
}

fn parse_octet(bytes: &[u8]) -> Option<u8> {
    let mut value: u16 = 0;
    let mut digit_count = 0;

    let starts_with_zero = bytes[0] == b'0';
    let mut index = 0;
    while index < bytes.len() && bytes[index] >= b'0' && bytes[index] <= b'9' {
        value = value * 10 + (bytes[index] - b'0') as u16;
        digit_count += 1;
        index += 1;

        if digit_count > 3 || value > 255 {
            return None; // Invalid octet
        }
    }

    if starts_with_zero && digit_count > 1 {
        return None; // Leading zero in octet
    }

    Some(value as u8)
}

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

#[cfg(test)]
mod tests {
    use super::Ipv4Address;

    #[test_case]
    fn test_address_parse() {
        assert_eq!(
            Ipv4Address::parse("192.168.0.1"),
            Some(Ipv4Address([192, 168, 0, 1]))
        );
        assert_eq!(
            Ipv4Address::parse("127.0.0.1"),
            Some(Ipv4Address([127, 0, 0, 1]))
        );

        assert_eq!(
            Ipv4Address::parse("0.0.0.0"),
            Some(Ipv4Address([0, 0, 0, 0]))
        );

        assert_eq!(Ipv4Address::parse("127.0.0.01"), None);
        assert_eq!(Ipv4Address::parse("www.example.net"), None);
        assert_eq!(Ipv4Address::parse("12.0.0.256"), None);
        assert_eq!(Ipv4Address::parse("192.168.1"), None);
        assert_eq!(Ipv4Address::parse("192.168.1."), None);
        assert_eq!(Ipv4Address::parse("192.168.1.1.1"), None);
        assert_eq!(Ipv4Address::parse(".127.0.0.1"), None);
        assert_eq!(Ipv4Address::parse("10.0..23"), None);
        assert_eq!(Ipv4Address::parse("10"), None);
        assert_eq!(Ipv4Address::parse(""), None);
    }
}
