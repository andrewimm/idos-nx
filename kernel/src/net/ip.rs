use super::packet::PacketHeader;

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

impl core::fmt::Display for IPV4Address {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(core::format_args!("{}.{}.{}.{}", self[0], self[1], self[2], self[3]))
    }
}

#[repr(C, packed)]
pub struct IPHeader {
    pub version_header_len: u8,
    pub diff_services: u8,
    pub total_length: u16,
    pub identification: u16,
    pub fragment: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16,
    pub source: IPV4Address,
    pub dest: IPV4Address,
}

impl IPHeader {
    pub fn new(source: IPV4Address, dest: IPV4Address, content_len: u16, ttl: u8, protocol_type: IPProtocolType) -> Self {
        let mut header = Self {
            version_header_len: 0x45,
            diff_services: 0,
            total_length: (20 + content_len).to_be(),
            identification: 0,
            fragment: 0,
            ttl, 
            protocol: protocol_type as u8,
            checksum: 0,
            source,
            dest,
        };
        header.checksum = header.compute_checksum();

        header
    }

    /// shorthand for new() using the UDP protocol
    pub fn new_udp(source: IPV4Address, dest: IPV4Address, content_len: u16, ttl: u8) -> Self {
        Self::new(source, dest, content_len, ttl, IPProtocolType::UDP)
    }

    pub fn new_tcp(source: IPV4Address, dest: IPV4Address, content_len: u16, ttl: u8) -> Self {
        Self::new(source, dest, content_len, ttl, IPProtocolType::TCP)
    }

    pub fn compute_checksum(&self) -> u16 {
        // cast header as 10 16-bit numbers
        let ptr = self as *const Self as *const u16;
        let len = 10;
        let slice = unsafe {
            core::slice::from_raw_parts(ptr, len)
        };

        let mut checksum = Checksum::new();
        for i in 0..slice.len() {
            checksum.add_u16(slice[i]);
        }
        checksum.compute()
    }
}

pub struct Checksum(u32);

impl Checksum {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn add_u16(&mut self, value: u16) {
        self.0 += value as u32;
    }

    pub fn compute(&self) -> u16 {
        let mut running_sum = self.0;
        let carry = running_sum >> 16;
        running_sum &= 0xffff;
        running_sum += carry;
        if running_sum & 0xffff0000 != 0 {
            running_sum += 1;
        }

        (!running_sum) as u16
    }
}

impl PacketHeader for IPHeader {}

#[repr(u8)]
pub enum IPProtocolType {
    TCP = 0x06,
    UDP = 0x11,
}

