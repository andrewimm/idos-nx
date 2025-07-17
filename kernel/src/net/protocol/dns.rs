use core::sync::atomic::{AtomicU16, Ordering};

use alloc::{string::String, vec::Vec};

use crate::net::socket::{get_ephemeral_port, port::SocketPort};

use super::packet::PacketHeader;

static DNS_PORT: AtomicU16 = AtomicU16::new(0);

pub fn get_dns_port() -> SocketPort {
    let load = DNS_PORT.load(Ordering::SeqCst);
    if load == 0 {
        let new_port = get_ephemeral_port().unwrap();

        DNS_PORT.store(*new_port, Ordering::SeqCst);
        new_port
    } else {
        SocketPort::new(load)
    }
}

#[repr(C, packed)]
pub struct DnsHeader {
    pub id: u16,
    pub flags: u16,
    pub question_count: u16,
    pub answer_count: u16,
    pub authority_count: u16,
    pub additional_count: u16,
}

impl DnsHeader {
    pub const FLAG_RESPONSE: u16 = 0x8000; // Query/Response
    pub const FLAG_AA: u16 = 0x0400; // Authoritative answer
    pub const FLAG_TC: u16 = 0x0200; // Truncated response
    pub const FLAG_RD: u16 = 0x0100; // Recursion desired
    pub const FLAG_RA: u16 = 0x0080; // Recursion available

    pub fn is_response(&self) -> bool {
        self.flags & Self::FLAG_RESPONSE != 0
    }

    pub fn build_query_header(id: u16, question_count: u16) -> Self {
        let flags = Self::FLAG_RD;
        Self {
            id: id.to_be(),
            flags: flags.to_be(),
            question_count: question_count.to_be(),
            answer_count: 0,
            authority_count: 0,
            additional_count: 0,
        }
    }

    pub fn build_query_packet(questions: &[DnsQuestion]) -> Vec<u8> {
        let expected_size =
            Self::get_size() + questions.iter().map(|q| q.name_length() + 4).sum::<usize>(); // include 4 bytes per question for type and class
        let mut packet = Vec::with_capacity(expected_size);
        let mut xid_bytes: [u8; 2] = [0; 2];
        crate::random::get_random_bytes(&mut xid_bytes);
        let xid: u16 = u16::from_le_bytes(xid_bytes);
        let header = Self::build_query_header(xid, questions.len() as u16);
        packet.extend_from_slice(&header.as_u8_buffer());

        for question in questions {
            match question {
                DnsQuestion::A(name) => {
                    packet.extend_from_slice(&name);
                    packet.extend_from_slice(&[0, 1]); // A record
                    packet.extend_from_slice(&[0, 1]); // Class IN
                }
                DnsQuestion::Cname(name) => {
                    packet.extend_from_slice(&name);
                    packet.extend_from_slice(&[0, 5]); // CNAME record
                    packet.extend_from_slice(&[0, 1]);
                }
            }
        }

        packet
    }
}

impl PacketHeader for DnsHeader {}

// it is expected that all names are already null-terminated
pub enum DnsQuestion {
    A(Vec<u8>),
    Cname(Vec<u8>),
}

impl DnsQuestion {
    pub fn name_length(&self) -> usize {
        match self {
            DnsQuestion::A(name) | DnsQuestion::Cname(name) => name.len() + 1, // extra byte for null terminator
        }
    }

    pub fn a_record(name: String) -> Self {
        // convert the domain name to DNS format
        let expected_length = name.len() + 2; // account for first label length and null terminator
        let mut encoded_name = Vec::with_capacity(expected_length);
        let labels = name.split('.');
        for label in labels {
            encoded_name.push(label.len() as u8);
            encoded_name.extend_from_slice(label.as_bytes());
        }
        encoded_name.push(0);
        DnsQuestion::A(encoded_name)
    }
}
