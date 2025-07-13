use crate::net::protocol::ipv4::IpProtocolType;

use super::super::checksum::{Checksum, IpChecksumHeader};
use super::super::ipv4::Ipv4Address;
use super::super::packet::PacketHeader;

#[repr(C, packed)]
pub struct TcpHeader {
    pub source_port: u16,
    pub dest_port: u16,
    /// Position of this data within the overall stream
    pub sequence_number: u32,
    /// Next sequence number expected by the sender, on an ACK packet
    pub ack_number: u32,
    /// size of the header in 32-bit words. Only the first 4 bits are used,
    /// the others are reserved.
    pub data_offset: u8,
    pub flags: u8,
    /// Number of bytes the receiver is willing to accept
    pub window_size: u16,
    /// UDP/TCP checksum
    pub checksum: u16,
    pub urgent_pointer: u16,
}

impl TcpHeader {
    pub const FLAG_FIN: u8 = 0x01;
    pub const FLAG_SYN: u8 = 0x02;
    pub const FLAG_RST: u8 = 0x04;
    pub const FLAG_ACK: u8 = 0x10;

    pub fn byte_size(&self) -> usize {
        (self.data_offset >> 4) as usize * 4
    }

    pub fn is_syn(&self) -> bool {
        self.flags & Self::FLAG_SYN != 0
    }

    pub fn is_ack(&self) -> bool {
        self.flags & Self::FLAG_ACK != 0
    }

    pub fn is_fin(&self) -> bool {
        self.flags & Self::FLAG_FIN != 0
    }

    pub fn is_rst(&self) -> bool {
        self.flags & Self::FLAG_RST != 0
    }

    /// TCP Packets build their checksum with a pseudo-IPV4 header that includes
    /// source and dest addresses.
    pub fn compute_checksum(
        &self,
        source_ip: Ipv4Address,
        dest_ip: Ipv4Address,
        data: &[u8],
    ) -> u16 {
        let mut data_length = data.len();
        if data_length & 1 != 0 {
            data_length += 1; // pad to even length
        }
        let checksum_header = IpChecksumHeader {
            source_ip,
            dest_ip,
            zeroes: 0,
            protocol: IpProtocolType::Tcp as u8,
            udp_length: ((Self::get_size() + data_length) as u16).to_be(),
        };

        let mut checksum = Checksum::new();
        for value in checksum_header.try_as_u16_buffer().unwrap() {
            checksum.add_u16(*value);
        }
        let header_slice = unsafe {
            let ptr = self as *const Self as *const u16;
            let len = Self::get_size() / 2;
            core::slice::from_raw_parts(ptr, len)
        };
        for value in header_slice {
            checksum.add_u16(*value);
        }

        let mut i = 0;
        while i < data_length {
            let word = if i + 1 < data_length {
                u16::from_be_bytes([data[i], data[i + 1]])
            } else {
                u16::from_be_bytes([data[i], 0]) // pad with zero if odd length
            };
            checksum.add_u16(word);
            i += 2;
        }

        checksum.compute()
    }
}

impl PacketHeader for TcpHeader {}
