use core::sync::atomic::Ordering;

use alloc::collections::VecDeque;
use idos_api::io::error::{IOError, IOResult};

use crate::task::map::get_task;

use super::{
    super::protocol::{ipv4::Ipv4Address, tcp::header::TcpHeader},
    connection::{Connection, ListenerConnections},
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
        remote_addr: Ipv4Address,
        tcp_header: &TcpHeader,
        data: &[u8],
    ) -> Option<(SocketId, Connection)> {
        let remote_port = SocketPort::new(tcp_header.source_port);
        match self.connections.find(remote_addr, remote_port) {
            Some(existing_conn_id) => match super::SOCKET_MAP.write().get_mut(&existing_conn_id) {
                Some(SocketType::TcpConnection(conn)) => {
                    conn.handle_packet(remote_addr, tcp_header, data);
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
                        let callback = self.pending_accept.pop_front().unwrap();
                        crate::kprintln!("CREATE NEW CONNECTION");
                        let socket_id =
                            SocketId::new(super::NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst));
                        let connection = Connection::new(self.local_port, remote_addr, remote_port);
                        self.connections.add(remote_addr, remote_port, socket_id);
                        crate::kprintln!("COMPLETE OP");
                        complete_op(callback, Ok(*socket_id));
                        return Some((socket_id, connection));
                    }
                }
            }
        }
        None
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
