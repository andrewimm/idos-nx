use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::RwLock;
use super::header::TCPHeader;
use super::super::ip::IPV4Address;
use super::super::socket::{SocketHandle, SocketPort};

#[derive(Copy, Clone)]
pub enum TCPState {
    /// Local host initiated a connection, and is waiting for ack
    SynSent,
    /// Local host received a SYN, and sent an ACK back
    SynReceived,
    Established,

    /// Received a FIN request, send one back, waiting for ACK
    LastAck,
}

#[derive(Copy, Clone)]
pub struct TCPConnection {
    pub state: TCPState,
    pub last_sequence_sent: u32,
    pub last_sequence_received: u32,
}

impl TCPConnection {
    pub fn new(seq: u32) -> Self {
        // TODO: this needs to be random
        let generated_sequence = 10;
        Self {
            state: TCPState::SynReceived,
            last_sequence_sent: generated_sequence,
            last_sequence_received: seq,
        }
    }

    pub fn is_connected(&self) -> bool {
        match self.state {
            TCPState::Established => true,
            _ => false,
        }
    }
}

/// The TCPAction enum encodes a number of actions that should be performed
/// when a TCP packet is received. Because TCP is basically a state machine,
/// there are a number of cases where a received packet triggers some new
/// behavior beyond the core send/receive loop
pub enum TCPAction {
    /// Send the packet to be read from the socket
    Enqueue,
    /// Throw away the packet, ie in case of a duplicate
    Discard,

    /// Close the socket without sending anything
    Close,
    /// Send a RST packet and close the connection
    Reset,
    /// Send a FIN/ACK to close
    FinAck,
    /// Mark the connection as established
    Connect,
}

pub fn action_for_tcp_packet(connection: &TCPConnection, header: &TCPHeader) -> TCPAction {
    if header.is_rst() {
        // no matter what state the connection is in, a reset closes it
        return TCPAction::Close;
    }
    match connection.state {
        TCPState::SynSent => {
            if !header.is_syn() || !header.is_ack() {
                return TCPAction::Reset;
            }
            // TODO: implement
            TCPAction::Discard
        },
        TCPState::SynReceived => {
            if !header.is_ack() {
                return TCPAction::Reset;
            }
            let ack = header.ack_number.to_be();
            if ack != connection.last_sequence_sent + 1 {
                return TCPAction::Reset;
            }
            TCPAction::Connect
        },

        TCPState::Established => {
            if header.is_fin() {
                return TCPAction::FinAck;
            }
            if header.is_syn() {
                return TCPAction::Reset;
            }
            TCPAction::Enqueue
        },

        TCPState::LastAck => {
            TCPAction::Close
        },
    }
}

pub struct TCPConnectionLookup {
    remote_ip: IPV4Address,
    remote_port: SocketPort,
    local_ip: IPV4Address,
    local_port: SocketPort,
    handle: SocketHandle,
}

static ESTABLISHED_CONNECTIONS: RwLock<BTreeMap<SocketPort, Vec<TCPConnectionLookup>>> = RwLock::new(BTreeMap::new());

/// Get the Socket Handle for an established connection between a local ip/port
/// and a remote ip/port
pub fn get_tcp_connection_socket(local_ip: IPV4Address, local_port: SocketPort, remote_ip: IPV4Address, remote_port: SocketPort) -> Option<SocketHandle> {
    let connections = ESTABLISHED_CONNECTIONS.read();
    let port_list = connections.get(&local_port)?;
    for lookup in port_list.iter() {
        if lookup.remote_ip != remote_ip {
            continue;
        }
        if lookup.remote_port != remote_port {
            continue;
        }
        return Some(lookup.handle);
    }
    None
}

/// Associate a local and remote endpoint pair with a newly opened socket, for
/// easy lookup
pub fn add_tcp_connection_lookup(local_ip: IPV4Address, local_port: SocketPort, remote_ip: IPV4Address, remote_port: SocketPort, handle: SocketHandle) {
    let lookup = TCPConnectionLookup {
        remote_ip,
        remote_port,
        local_ip,
        local_port,
        handle,
    };
    let mut connections = ESTABLISHED_CONNECTIONS.write();
    if connections.get(&local_port).is_none() {
        let mut list = Vec::with_capacity(1);
        list.push(lookup);
        connections.insert(local_port, list);
        return;
    }
    connections.get_mut(&local_port).unwrap().push(lookup);
}

pub fn remove_tcp_connection_lookup(local_ip: IPV4Address, local_port: SocketPort, remote_ip: IPV4Address, remote_port: SocketPort) {
    let mut connections = ESTABLISHED_CONNECTIONS.write();
    match connections.get_mut(&local_port) {
        Some(port_list) => {
            for i in 0..port_list.len() {
                let lookup = port_list.get(i).unwrap();
                if lookup.remote_ip != remote_ip {
                    continue;
                }
                if lookup.remote_port != remote_port {
                    continue;
                }
                port_list.remove(i);
                return;
            }
        },
        None => return,
    }
}

