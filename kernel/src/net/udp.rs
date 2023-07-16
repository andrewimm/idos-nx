use alloc::vec::Vec;

use super::ethernet::EthernetFrame;
use super::ip::{IPV4Address, IPHeader};
use super::packet::PacketHeader;

#[repr(C, packed)]
pub struct UDPHeader {
    pub source_port: u16,
    pub dest_port: u16,
    pub length: u16,
    pub checksum: u16,
}

impl PacketHeader for UDPHeader {}

impl UDPHeader {
    pub fn new(source_ip: IPV4Address, source_port: u16, dest_ip: IPV4Address, dest_port: u16, data_size: usize) -> Self {
        let length = (8 + data_size) as u16;
        let checksum = 0u16;

        Self {
            source_port: source_port.to_be(),
            dest_port: dest_port.to_be(),
            length: length.to_be(),
            checksum: checksum.to_be(),
        }
    }
}

pub fn create_datagram(source_ip: IPV4Address, source_port: u16, dest_ip: IPV4Address, dest_port: u16, data: &[u8]) -> Vec<u8> {
    let total_size = data.len() + UDPHeader::get_size() + IPHeader::get_size();
    let mut datagram_vec = Vec::new();
    for i in 0..total_size {
        datagram_vec.push(0);
    }
    let datagram_buffer = datagram_vec.as_mut_slice();

    // copy payload
    let data_start = total_size - data.len();
    datagram_buffer[data_start..].copy_from_slice(data);
    // copy UDP header
    let udp_header = UDPHeader::new(source_ip, source_port, dest_ip, dest_port, data.len());
    let udp_header_space = &mut datagram_buffer[..data_start];
    let udp_start = udp_header.copy_to_buffer(udp_header_space);
    let udp_size = (UDPHeader::get_size() + data.len()) as u16;
    // copy IP header
    let ip_header = IPHeader::new_udp(source_ip, dest_ip, udp_size, 127);
    let ip_header_space = &mut datagram_buffer[..udp_start];
    let ip_start = ip_header.copy_to_buffer(ip_header_space);
    assert_eq!(ip_start, 0, "Should not have extra space in the datagram buffer");

    datagram_vec
}
