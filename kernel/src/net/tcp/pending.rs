//! When a TCP packet arrives at a valid listening socket, the net stack checks
//! if a connection between the local client and the remote host already
//! exists. If no connection has been established yet, the initiating packet
//! becomes a pending connection - a request for connection that can be
//! accept()-ed by the local client.

use alloc::collections::{BTreeMap, VecDeque};
use crate::time::system::{Timestamp, get_system_time};
use spin::RwLock;
use super::super::ip::IPV4Address;
use super::super::socket::SocketPort;

/// Stores the information necessary to initiate a TCP connection, pulling the
/// relevant data from the initial SYN request
#[derive(Copy, Clone)]
pub struct PendingConnection {
    established: Timestamp,
    pub seq_received: u32,

    pub remote_ip: IPV4Address,
    pub remote_port: SocketPort,
    pub local_ip: IPV4Address,
    pub local_port: SocketPort,
}

/// All pending request are stored in this global map, keyed by the local port
/// that they attempted to connect to.
static PENDING_CONNECTIONS: RwLock<BTreeMap<SocketPort, VecDeque<PendingConnection>>> = RwLock::new(BTreeMap::new());

/// Convert data from a TCP SYN packet into a pending connection, and add it to
/// the queue.
pub fn add_pending_connection(remote_ip: IPV4Address, remote_port: SocketPort, local_ip: IPV4Address, local_port: SocketPort, seq_received: u32) {
    let established = get_system_time().to_timestamp();

    let pending = PendingConnection {
        established,
        seq_received,
        remote_ip,
        remote_port,
        local_ip,
        local_port,
    };
    let mut connections = PENDING_CONNECTIONS.write();
    if connections.get(&local_port).is_none() {
        let mut list = VecDeque::with_capacity(1);
        list.push_back(pending);
        connections.insert(local_port, list);
        return;
    }
    connections.get_mut(&local_port).unwrap().push_back(pending);
}

/// Pull the earliest pending connection request for a given port. This is the
/// first step for the socket accept() action
pub fn accept_pending_connection(port: SocketPort, timeout: Option<u32>) -> Option<PendingConnection> {
    let mut connections = PENDING_CONNECTIONS.write();
    let list = connections.get_mut(&port)?;
    list.pop_front()
}
