use super::{
    super::protocol::{ipv4::Ipv4Address, tcp::header::TcpHeader},
    port::SocketPort,
};

pub struct UdpListener {}

impl UdpListener {
    pub fn new(port: SocketPort) -> Self {
        Self {}
    }

    pub fn handle_packet(&self, remote_addr: Ipv4Address, remote_port: u16, data: &[u8]) {}
}

pub struct TcpListener {}

impl TcpListener {
    pub fn new(port: SocketPort) -> Self {
        Self {}
    }

    pub fn handle_packet(&self, remote_addr: Ipv4Address, tcp_header: &TcpHeader, data: &[u8]) {}
}
