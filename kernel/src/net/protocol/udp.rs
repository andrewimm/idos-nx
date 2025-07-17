use alloc::vec::Vec;

use super::{
    checksum::{Checksum, IpChecksumHeader},
    ipv4::{IpProtocolType, Ipv4Address, Ipv4Header},
    packet::PacketHeader,
};

#[repr(C, packed)]
pub struct UdpHeader {
    pub source_port: u16,
    pub dest_port: u16,
    pub length: u16,
    pub checksum: u16,
}

impl UdpHeader {
    pub fn new(source_port: u16, dest_port: u16, data_size: usize) -> Self {
        let length = (8 + data_size) as u16;
        let checksum = 0u16;

        Self {
            source_port: source_port.to_be(),
            dest_port: dest_port.to_be(),
            length: length.to_be(),
            checksum: checksum.to_be(),
        }
    }

    pub fn compute_checksum(
        &self,
        source_ip: Ipv4Address,
        dest_ip: Ipv4Address,
        data: &[u8],
    ) -> u16 {
        let mut data_length = data.len();
        if data_length & 1 != 0 {
            data_length += 1;
        }
        let checksum_header = IpChecksumHeader {
            source_ip,
            dest_ip,
            zeroes: 0,
            protocol: IpProtocolType::Udp as u8,
            udp_length: ((UdpHeader::get_size() + data.len()) as u16).to_be(),
        };
        let mut checksum = Checksum::new();
        for value in checksum_header.try_as_u16_buffer().unwrap().iter() {
            checksum.add_u16(*value);
        }

        checksum.add_u16(self.source_port);
        checksum.add_u16(self.dest_port);
        checksum.add_u16(self.length);

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

impl PacketHeader for UdpHeader {}

pub fn create_datagram(
    source_ip: Ipv4Address,
    source_port: u16,
    dest_ip: Ipv4Address,
    dest_port: u16,
    data: &[u8],
) -> Vec<u8> {
    let total_size = data.len() + UdpHeader::get_size() + Ipv4Header::get_size();
    let mut datagram_vec = Vec::with_capacity(total_size);
    for _ in 0..total_size {
        datagram_vec.push(0);
    }
    let datagram_buffer = datagram_vec.as_mut_slice();

    // copy payload
    let data_start_offset = total_size - data.len();
    datagram_buffer[data_start_offset..].copy_from_slice(data);
    // create the UDP header
    let mut udp_header = UdpHeader::new(source_port, dest_port, data.len());
    let checksum = udp_header.compute_checksum(source_ip, dest_ip, data);
    udp_header.checksum = checksum;
    let udp_header_space = &mut datagram_buffer[..data_start_offset];
    let udp_start_offset = udp_header.copy_to_u8_buffer(udp_header_space);
    let udp_size = (UdpHeader::get_size() + data.len()) as u16;
    // create the IPV4 header
    let ip_header = Ipv4Header::new_udp(
        source_ip, dest_ip, udp_size, 64, // default TTL
    );
    let ip_header_space = &mut datagram_buffer[..udp_start_offset];
    let ip_start_offset = ip_header.copy_to_u8_buffer(ip_header_space);

    assert_eq!(
        ip_start_offset, 0,
        "Should not have extra space in the datagram buffer"
    );

    datagram_vec
}
