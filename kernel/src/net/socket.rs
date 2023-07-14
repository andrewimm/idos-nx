

use alloc::collections::BTreeMap;
use spin::RwLock;

use crate::net::{ip::IPProtocolType, udp::UDPHeader};

use super::{ip::{IPV4Address, IPHeader}, packet::PacketHeader};

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct SocketPort(u16);

impl core::ops::Deref for SocketPort {
    type Target = u16;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// After a socket is created, it needs to be bound to addresses / ports.
/// All sockets have a local endpoint, even if the local port is arbitrarily
/// selected by the kernel.
/// Sockets that are meant for receiving packets have no bound remote endpoint.
/// Those sockets will have a remote ip / port consisting of all zeroes.
pub struct SocketBinding {
    local_ip: IPV4Address,
    local_port: SocketPort,

    remote_ip: IPV4Address,
    remote_port: SocketPort,
}

pub struct OpenSocket {
}

#[derive(Copy, Clone)]
pub struct SocketHandle(u32);

static OPEN_SOCKETS: RwLock<BTreeMap<SocketHandle, OpenSocket>> = RwLock::new(BTreeMap::new());

pub fn receive_ip_packet(raw: &[u8]) {
    let ip_header = match IPHeader::from_buffer(raw) {
        Some(header) => header,
        None => return,
    };
    let remainder = &raw[IPHeader::get_size()..];
    crate::kprintln!("IP packet from {}", ip_header.source);
    if ip_header.protocol == IPProtocolType::TCP as u8 {
        crate::kprintln!("TCP not supported yet");
    } else if ip_header.protocol == IPProtocolType::UDP as u8 {
        let udp_header = match UDPHeader::from_buffer(remainder) {
            Some(header) => header,
            None => return,
        };
        let dest_port = udp_header.dest_port.to_be();
        crate::kprintln!("UDP Datagram to port {}", dest_port.clone());
    }
}

