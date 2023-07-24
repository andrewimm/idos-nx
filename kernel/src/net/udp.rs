use alloc::vec::Vec;

use super::ip::{IPV4Address, IPHeader, IPProtocolType, Checksum};
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

    pub fn compute_checksum(&self, source_ip: IPV4Address, dest_ip: IPV4Address, data: &[u8]) -> u16 {
        let mut data_length = data.len();
        if data_length & 1 != 0 {
            data_length += 1;
        }
        let checksum_header = IPChecksumHeader {
            source_ip,
            dest_ip,
            zeroes: 0,
            protocol: IPProtocolType::UDP as u8,
            udp_length: ((UDPHeader::get_size() + data_length) as u16).to_be(),
        };
        let mut checksum = Checksum::new();
        for value in checksum_header.as_u16_buffer().iter() {
            checksum.add_u16(*value);
        }

        checksum.add_u16(self.source_port);
        checksum.add_u16(self.dest_port);
        checksum.add_u16(self.length);

        let mut i = 0;
        while i < data.len() {
            let low = data[i];
            let high = if i + 1 >= data.len() {
                0
            } else {
                data[i + 1]
            };
            let value = (low as u16) | ((high as u16) << 8);
            checksum.add_u16(value);
            i += 2;
        }

        checksum.compute()
    }
}

#[repr(C, packed)]
pub struct IPChecksumHeader {
    pub source_ip: IPV4Address,
    pub dest_ip: IPV4Address,
    pub zeroes: u8,
    pub protocol: u8,
    pub udp_length: u16,
}

impl IPChecksumHeader {
    pub fn as_u16_buffer(&self) -> &[u16] {
        let ptr = self as *const Self as *const u16;
        let size = core::mem::size_of::<Self>() / 2;
        unsafe { core::slice::from_raw_parts(ptr, size) }
    }
}

pub fn create_datagram(source_ip: IPV4Address, source_port: u16, dest_ip: IPV4Address, dest_port: u16, data: &[u8]) -> Vec<u8> {
    let total_size = data.len() + UDPHeader::get_size() + IPHeader::get_size();
    let mut datagram_vec = Vec::with_capacity(total_size);
    for _ in 0..total_size {
        datagram_vec.push(0);
    }
    let datagram_buffer = datagram_vec.as_mut_slice();

    // copy payload
    let data_start = total_size - data.len();
    datagram_buffer[data_start..].copy_from_slice(data);
    // copy UDP header
    let mut udp_header = UDPHeader::new(source_port, dest_port, data.len());
    let checksum = udp_header.compute_checksum(source_ip, dest_ip, data);
    udp_header.checksum = checksum;
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
