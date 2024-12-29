use super::super::checksum::{Checksum, IPChecksumHeader};
use super::super::ip::{IPProtocolType, IPV4Address, IPV4Header};
use super::super::packet::PacketHeader;
use super::super::socket::SocketPort;
use alloc::vec::Vec;

pub const TCP_FLAG_CWR: u8 = 0x80;
pub const TCP_FLAG_ECE: u8 = 0x40;
pub const TCP_FLAG_URG: u8 = 0x20;
pub const TCP_FLAG_ACK: u8 = 0x10;
pub const TCP_FLAG_PSH: u8 = 0x08;
pub const TCP_FLAG_RST: u8 = 0x04;
pub const TCP_FLAG_SYN: u8 = 0x02;
pub const TCP_FLAG_FIN: u8 = 0x01;

#[repr(C, packed)]
pub struct TCPHeader {
    pub source_port: u16,
    pub dest_port: u16,
    pub sequence_number: u32,
    pub ack_number: u32,
    /// size of the header in 32-bit words
    pub data_offset: u8,
    pub flags: u8,
    pub window_size: u16,
    pub checksum: u16,
    pub urgent_pointer: u16,
}

impl TCPHeader {
    pub fn byte_size(&self) -> usize {
        (self.data_offset >> 4) as usize * 4
    }

    pub fn get_destination_port(&self) -> SocketPort {
        SocketPort::new(self.dest_port.to_be())
    }

    pub fn get_source_port(&self) -> SocketPort {
        SocketPort::new(self.source_port.to_be())
    }

    pub fn is_syn(&self) -> bool {
        self.flags & TCP_FLAG_SYN != 0
    }

    pub fn is_ack(&self) -> bool {
        self.flags & TCP_FLAG_ACK != 0
    }

    pub fn is_fin(&self) -> bool {
        self.flags & TCP_FLAG_FIN != 0
    }

    pub fn is_rst(&self) -> bool {
        self.flags & TCP_FLAG_RST != 0
    }

    pub fn compute_checksum(
        &self,
        source_ip: IPV4Address,
        dest_ip: IPV4Address,
        data: &[u8],
    ) -> u16 {
        let mut data_length = data.len();
        if data_length & 1 != 0 {
            data_length += 1;
        }
        let checksum_header = IPChecksumHeader {
            source_ip,
            dest_ip,
            zeroes: 0,
            protocol: IPProtocolType::TCP as u8,
            udp_length: ((TCPHeader::get_size() + data_length) as u16).to_be(),
        };
        let mut checksum = Checksum::new();
        for value in checksum_header.try_as_u16_buffer().unwrap().iter() {
            checksum.add_u16(*value);
        }

        let header_slice = unsafe {
            let ptr = self as *const TCPHeader as *const u16;
            let len = TCPHeader::get_size() / 2;
            core::slice::from_raw_parts(ptr, len)
        };
        for value in header_slice.iter() {
            checksum.add_u16(*value);
        }

        let mut i = 0;
        while i < data.len() {
            let low = data[i];
            let high = if i + 1 >= data.len() { 0 } else { data[i + 1] };
            let value = (low as u16) | ((high as u16) << 8);
            checksum.add_u16(value);
            i += 2;
        }

        checksum.compute()
    }
}

impl PacketHeader for TCPHeader {}

pub fn create_tcp_packet(
    source_ip: IPV4Address,
    source_port: SocketPort,
    dest_ip: IPV4Address,
    dest_port: SocketPort,
    seq_number: u32,
    ack_number: u32,
    flags: u8,
    data: &[u8],
) -> Vec<u8> {
    let total_size = TCPHeader::get_size() + IPV4Header::get_size() + data.len();
    let mut packet_vec = Vec::with_capacity(total_size);
    for _ in 0..total_size {
        packet_vec.push(0);
    }
    let packet_buffer = packet_vec.as_mut_slice();
    let mut tcp_header = TCPHeader {
        source_port: source_port.to_be(),
        dest_port: dest_port.to_be(),
        sequence_number: seq_number.to_be(),
        ack_number: ack_number.to_be(),
        data_offset: ((TCPHeader::get_size() / 4) as u8) << 4,
        flags,
        window_size: 0xffff,
        checksum: 0,
        urgent_pointer: 0,
    };
    tcp_header.checksum = tcp_header.compute_checksum(source_ip, dest_ip, data);
    let data_start = total_size - data.len();
    packet_buffer[data_start..].copy_from_slice(data);
    let tcp_start = tcp_header.copy_to_u8_buffer(&mut packet_buffer[..data_start]);
    let tcp_size = (TCPHeader::get_size() + data.len()) as u16;
    let ip_header = IPV4Header::new_tcp(source_ip, dest_ip, tcp_size, 127);
    let ip_start = ip_header.copy_to_u8_buffer(&mut packet_buffer[..tcp_start]);
    assert_eq!(
        ip_start, 0,
        "Should not have extra space in the packet buffer"
    );

    packet_vec
}
