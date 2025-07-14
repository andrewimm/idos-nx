//! Sockets are kept as close to the original BSD definition as makes sense.
//! They are a way to stream data to and from a remote port, using either TCP
//! or UDP protocols.
//! Sockets also naturally fit into the AsyncIO semantics of IDOS.
//! First, a socket is created. This just generates a handle with no changes
//! within the net stack.
//! Next, the socket is bound, just like binding a file handle to a path. A
//! socket can either be bound locally (eg, 0.0.0.0:8080), or remotely.
//!
//! For UDP Sockets:
//! If the port was local and not already in use, the socket can issue `read`
//! IO ops to the socket handle. Each read will block until a packet is received
//! on the port. Each write contains the destionation address and port in the
//! first 6 bytes, followed by the data that is instantly sent.
//!
//! For TCP Sockets:
//! If the port was local, the socket becomes a listener. Issuing a `read` op
//! will block until a connection is made. Once a connection is established,
//! a new socket handle will be generated and attached to the Task. The 32-bit
//! ID of that socket handle will be returned, and the Task can use that the
//! same way as a remote connection.
//! For remote TCP connections, an opened socket will automatically initiate
//! the SYN/ACK handshake. The open/bind operation will block until the
//! handshake is complete. If successful, read/write IO ops will send data to
//! and from the remote location.
//!
//! This means there are three different classes of sockets:
//!  - UDP, bound to a local port
//!  - TCP Listener, bound to a local port
//!  - TCP Connection, with a local and remote address
//! Each of these is represented by a socket handle, and modified through a
//! socket IO provider.

pub mod binding;
pub mod connection;
pub mod listen;
pub mod port;

use alloc::collections::BTreeMap;
use spin::RwLock;

use self::{
    connection::{Connection, ConnectionId},
    listen::{TcpListener, UdpListener},
    port::SocketPort,
};
use super::protocol::ipv4::Ipv4Address;

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct SocketId(u32);

impl SocketId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

/// Mapping of local ports to sockets. This is used when a new packet arrives,
/// to determine how it should be handled.
static ACTIVE_CONNECTIONS: RwLock<BTreeMap<SocketPort, SocketId>> = RwLock::new(BTreeMap::new());

enum SocketType {
    Udp(UdpListener),
    TcpListener(TcpListener),
    TcpConnection(Connection),
}

/// Map of all active sockets
static SOCKET_MAP: RwLock<BTreeMap<SocketId, SocketType>> = RwLock::new(BTreeMap::new());

pub enum SocketProtocol {
    Udp,
    Tcp,
}

/// When a task issues a bind operation on a socket handle, this method contains
/// the logic used to create the socket.
/// Like all async IO operations, it returns an optional result. If the op can
/// complete immediately, like creating a local UDP binding, it returns `Some`
/// Result. If the operation must wait for a remote connection, it returns
/// `None` and uses the async callback info to resolve the IO operation when
/// complete.
pub fn socket_io_bind(
    protocol: SocketProtocol,
    addr: Ipv4Address,
    port: u16,
) -> Option<Result<SocketId, ()>> {
    None
}
