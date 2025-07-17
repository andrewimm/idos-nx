use alloc::vec::Vec;

use crate::net::protocol::ipv4::IpProtocolType;

use super::super::super::socket::port::SocketPort;
use super::super::checksum::{Checksum, IpChecksumHeader};
use super::super::ipv4::{Ipv4Address, Ipv4Header};
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
            udp_length: ((Self::get_size() + data.len()) as u16).to_be(),
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

    pub fn create_packet(
        source_ip: Ipv4Address,
        source_port: SocketPort,
        dest_ip: Ipv4Address,
        dest_port: SocketPort,
        seq_number: u32,
        ack_number: u32,
        flags: u8,
        data: &[u8],
    ) -> Vec<u8> {
        let total_size = Ipv4Header::get_size() + Self::get_size() + data.len();
        let mut packet_vec = Vec::with_capacity(total_size);
        for _ in 0..total_size {
            packet_vec.push(0);
        }
        let packet_buffer = packet_vec.as_mut_slice();
        let mut tcp_header = Self {
            source_port: (*source_port).to_be(),
            dest_port: (*dest_port).to_be(),
            sequence_number: seq_number.to_be(),
            ack_number: ack_number.to_be(),
            data_offset: ((Self::get_size() / 4) as u8) << 4,
            flags,
            window_size: 0xffff,
            checksum: 0,
            urgent_pointer: 0,
        };
        tcp_header.checksum = tcp_header.compute_checksum(source_ip, dest_ip, data);
        let data_start = total_size - data.len();
        packet_buffer[data_start..].copy_from_slice(data);
        let tcp_start = tcp_header.copy_to_u8_buffer(&mut packet_buffer[..data_start]);
        let tcp_size = (Self::get_size() + data.len()) as u16;
        let ip_header = Ipv4Header::new_tcp(source_ip, dest_ip, tcp_size, 127);
        let ip_start = ip_header.copy_to_u8_buffer(&mut packet_buffer[..tcp_start]);
        assert_eq!(
            ip_start, 0,
            "Should not have extra space in the packet buffer"
        );

        packet_vec
    }
}

impl PacketHeader for TcpHeader {}
