use alloc::collections::BTreeMap;
use idos_api::io::error::IOResult;

use super::{
    super::protocol::{ipv4::Ipv4Address, tcp::header::TcpHeader},
    port::SocketPort,
    AsyncCallback, SocketId,
};

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct RemoteEndpoint {
    pub address: Ipv4Address,
    pub port: SocketPort,
}

pub struct ListenerConnections {
    lookup: BTreeMap<RemoteEndpoint, SocketId>,
}

impl ListenerConnections {
    pub fn new() -> Self {
        Self {
            lookup: BTreeMap::new(),
        }
    }

    pub fn add(
        &mut self,
        remote_address: Ipv4Address,
        remote_port: SocketPort,
        socket_id: SocketId,
    ) {
        let endpoint = RemoteEndpoint {
            address: remote_address,
            port: remote_port,
        };
        self.lookup.insert(endpoint, socket_id);
    }

    pub fn remove(
        &mut self,
        remote_address: Ipv4Address,
        remote_port: SocketPort,
    ) -> Option<SocketId> {
        let endpoint = RemoteEndpoint {
            address: remote_address,
            port: remote_port,
        };
        self.lookup.remove(&endpoint)
    }

    pub fn find(&self, remote_address: Ipv4Address, remote_port: SocketPort) -> Option<SocketId> {
        let endpoint = RemoteEndpoint {
            address: remote_address,
            port: remote_port,
        };
        self.lookup.get(&endpoint).copied()
    }
}

pub struct Connection {
    local_port: SocketPort,
    remote_address: Ipv4Address,
    remote_port: SocketPort,
}

impl Connection {
    pub fn new(
        local_port: SocketPort,
        remote_address: Ipv4Address,
        remote_port: SocketPort,
    ) -> Self {
        Self {
            local_port,
            remote_address,
            remote_port,
        }
    }

    pub fn handle_packet(&self, remote_addr: Ipv4Address, tcp_header: &TcpHeader, data: &[u8]) {}

    pub fn read(&self, buffer: &mut [u8], callback: AsyncCallback) -> Option<IOResult> {
        None
    }
}
