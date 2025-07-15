use core::sync::atomic::Ordering;

use alloc::collections::{BTreeMap, VecDeque};
use idos_api::io::error::{IOError, IOResult};

use crate::{net::protocol::packet::PacketHeader, task::map::get_task};

use super::{
    super::protocol::{
        ipv4::Ipv4Address,
        tcp::{connection::TcpConnection, header::TcpHeader},
    },
    super::resident::net_send,
    port::SocketPort,
    AsyncCallback, SocketId, SocketType,
};

pub struct UdpListener {}

impl UdpListener {
    pub fn new(port: SocketPort) -> Self {
        Self {}
    }

    pub fn handle_packet(&self, remote_addr: Ipv4Address, remote_port: u16, data: &[u8]) {}

    /// Block until the next packet is received on this UDP listener.
    /// If the packet is open, incoming reads will be queued up and can be
    /// immediately resolved. Otherwise, the method will return and the next
    /// incoming packet will use the async callback info to resolve the read
    /// operation.
    pub fn read(&self, buffer: &mut [u8], callback: AsyncCallback) -> Option<IOResult> {
        Some(Err(IOError::Unknown))
    }
}

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

pub struct TcpListener {
    local_port: SocketPort,
    connections: ListenerConnections,
    pending_syn: VecDeque<(Ipv4Address, SocketPort)>,
    pending_accept: VecDeque<AsyncCallback>,
}

impl TcpListener {
    pub fn new(port: SocketPort) -> Self {
        Self {
            local_port: port,
            connections: ListenerConnections::new(),
            pending_syn: VecDeque::new(),
            pending_accept: VecDeque::new(),
        }
    }

    pub fn handle_packet(
        &mut self,
        local_addr: Ipv4Address,
        remote_addr: Ipv4Address,
        tcp_header: &TcpHeader,
        data: &[u8],
    ) -> Option<(SocketId, TcpConnection)> {
        let remote_port = SocketPort::new(u16::from_be(tcp_header.source_port));
        match self.connections.find(remote_addr, remote_port) {
            Some(existing_conn_id) => match super::SOCKET_MAP.write().get_mut(&existing_conn_id) {
                Some(SocketType::TcpConnection(conn)) => {
                    conn.handle_packet(local_addr, remote_addr, tcp_header, data);
                }
                _ => {}
            },
            None => {
                if tcp_header.is_syn() {
                    // If the packet is a SYN, we queue it for later processing
                    crate::kprintln!("INCOMING SYN PACKET");
                    if self.pending_accept.is_empty() {
                        self.pending_syn.push_back((remote_addr, remote_port));
                    } else {
                        // If we have a pending accept, we can immediately process the SYN
                        //let callback = self.pending_accept.pop_front().unwrap();
                        crate::kprintln!("CREATE NEW CONNECTION");
                        return Some(self.init_connection(
                            local_addr,
                            remote_addr,
                            remote_port,
                            u32::from_be(tcp_header.sequence_number),
                        ));
                    }
                }
            }
        }
        None
    }

    pub fn init_connection(
        &mut self,
        local_addr: Ipv4Address,
        remote_addr: Ipv4Address,
        remote_port: SocketPort,
        last_seq: u32,
    ) -> (SocketId, TcpConnection) {
        let is_outbound = last_seq == 0;
        let socket_id = SocketId::new(super::NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst));
        let mut connection =
            TcpConnection::new(self.local_port, remote_addr, remote_port, is_outbound);
        connection.last_sequence_received = last_seq;
        self.connections.add(remote_addr, remote_port, socket_id);
        let flags = if is_outbound {
            TcpHeader::FLAG_SYN
        } else {
            TcpHeader::FLAG_SYN | TcpHeader::FLAG_ACK
        };
        crate::kprintln!("SEND THE PACKET");
        let response = TcpHeader::create_packet(
            local_addr,
            self.local_port,
            remote_addr,
            remote_port,
            connection.last_sequence_sent,
            connection.last_sequence_received + 1,
            flags,
            &[],
        );
        net_send(remote_addr, response);

        (socket_id, connection)
    }

    /// Accept a new connection on this TCP listener.
    /// Incoming SYN packets are queued. An accept call will complete the
    /// handshake. Regardless of whether a connection has been initiated before
    /// this method is called, it will always be an async process and will
    /// never immediately return an `IOResult>.
    pub fn accept(&mut self, buffer: &mut [u8], callback: AsyncCallback) -> Option<IOResult> {
        if self.pending_syn.is_empty() {
            self.pending_accept.push_back(callback);
            return None;
        }
        None
    }
}

fn complete_op(callback: AsyncCallback, result: IOResult) {
    let (task_id, io_index, op_id) = callback;
    let task_lock = match get_task(task_id) {
        Some(lock) => lock,
        None => return,
    };
    crate::kprintln!("COMPLETE");
    let io_entry = task_lock.read().async_io_complete(io_index);
    crate::kprintln!("COMPLETION COMPLETE?");
    if let Some(entry) = io_entry {
        entry.inner().async_complete(op_id, result);
    }
}
