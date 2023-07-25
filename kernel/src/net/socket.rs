use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use crate::task::actions::io::{open_path, close_file, write_file};

use super::ip::{IPProtocolType, IPV4Address, IPHeader};
use super::packet::PacketHeader;
use super::udp::{UDPHeader, create_datagram};
use super::error::NetError;
use super::ethernet::EthernetFrame;
use super::with_active_device;
use super::arp::resolve_mac_from_ip;
use super::tcp::connection::{TCPAction, TCPConnection, TCPState, action_for_tcp_packet, get_tcp_connection_socket, add_tcp_connection_lookup};
use super::tcp::header::{TCPHeader, create_syn_ack};
use super::tcp::pending::{accept_pending_connection, add_pending_connection};

#[derive(Copy, Clone, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct SocketPort(u16);

impl SocketPort {
    pub fn new(port: u16) -> Self {
        Self(port)
    }
}

impl core::ops::Deref for SocketPort {
    type Target = u16;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::fmt::Display for SocketPort {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!(":{}", self.0))
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

impl SocketBinding {
    pub fn empty() -> Self {
        Self {
            local_ip: IPV4Address([0, 0, 0, 0]),
            local_port: SocketPort(0),
            remote_ip: IPV4Address([0, 0, 0, 0]),
            remote_port: SocketPort(0),
        }
    }

    pub fn has_remote_binding(&self) -> bool {
        if *self.remote_ip == [0, 0, 0, 0] {
            return false;
        }
        *self.remote_port != 0
    }
}

pub struct OpenSocket {
    binding: SocketBinding,
    protocol: SocketProtocol,
    tcp_connection: Option<TCPConnection>,
}

#[derive(Copy, Clone)]
pub enum SocketProtocol {
    UDP,
    TCP,
}

impl OpenSocket {
    pub fn new_tcp() -> Self {
        Self {
            binding: SocketBinding::empty(),
            protocol: SocketProtocol::TCP,
            tcp_connection: None,
        }
    }

    pub fn bind(&mut self, local_ip: IPV4Address, local_port: SocketPort, remote_ip: IPV4Address, remote_port: SocketPort) {
        self.binding.local_ip = local_ip;
        self.binding.local_port = local_port;
        self.binding.remote_ip = remote_ip;
        self.binding.remote_port = remote_port;
    }
    
    pub fn set_tcp_connection(&mut self, connection: TCPConnection) {
        self.tcp_connection.replace(connection);
    }

    pub fn is_connected(&self) -> bool {
        match self.protocol {
            SocketProtocol::UDP => return true,
            SocketProtocol::TCP => (),
        }

        match &self.tcp_connection {
            Some(conn) => conn.is_connected(),
            None => return false,
        }
    }

    pub fn create_packet(&self, payload: &[u8]) -> Vec<u8> {
        match self.protocol {
            SocketProtocol::UDP => {
                create_datagram(self.binding.local_ip, *self.binding.local_port, self.binding.remote_ip, *self.binding.remote_port, payload)
            },
            SocketProtocol::TCP => {
                //create_tcp_packet(payload);
                panic!("TCP not working");
            },
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SocketHandle(u32);

static OPEN_SOCKETS: RwLock<BTreeMap<SocketHandle, OpenSocket>> = RwLock::new(BTreeMap::new());
/// Easily look up listening sockets by their local port (this excludes TCP connections)
static SOCKETS_BY_PORT: RwLock<BTreeMap<SocketPort, SocketHandle>> = RwLock::new(BTreeMap::new());

static NEXT_HANDLE: AtomicU32 = AtomicU32::new(1);

pub fn get_socket_on_port(port: SocketPort) -> Option<SocketHandle> {
    SOCKETS_BY_PORT.read().get(&port).copied()
}

/// Ephemeral ports are allocated temporarily on a local port while a
/// connection remains open. According to the IANA, they should be allocated
/// between 49152 and 65535
pub fn get_ephemeral_port() -> Option<SocketPort> {
    let mut query: u32 = 49152;
    while query <= 65535 {
        let port = SocketPort(query as u16);
        if !SOCKETS_BY_PORT.read().contains_key(&port) {
            return Some(port);
        }
        query += 1;
    }
    // All ephemeral ports are in use?!
    None
}

pub fn receive_ip_packet(raw: &[u8]) {
    let ip_header = match IPHeader::from_buffer(raw) {
        Some(header) => header,
        None => return,
    };
    let remainder = &raw[IPHeader::get_size()..];
    crate::kprintln!("IP packet from {}", ip_header.source);
    if ip_header.protocol == IPProtocolType::TCP as u8 {
        handle_incoming_tcp(ip_header.source, ip_header.dest, remainder);
    } else if ip_header.protocol == IPProtocolType::UDP as u8 {
        let udp_header = match UDPHeader::from_buffer(remainder) {
            Some(header) => header,
            None => return,
        };
        let dest_port = udp_header.dest_port.to_be();
        crate::kprintln!("UDP Datagram to port {}", dest_port.clone());

        if dest_port == 68 {
            super::dhcp::handle_incoming_packet(&remainder[UDPHeader::get_size()..]);
        }
    }
}

fn insert_socket(socket: OpenSocket) -> SocketHandle {
    let handle = SocketHandle(NEXT_HANDLE.fetch_add(1, Ordering::SeqCst));
    OPEN_SOCKETS.write().insert(handle, socket);
    handle
}

/// Create an unbound socket. A socket must be bound to a local or remote
/// address before it can be used for anything useful.
pub fn create_socket(protocol: SocketProtocol) -> SocketHandle {
    let socket = OpenSocket {
        binding: SocketBinding::empty(),
        protocol,
        tcp_connection: None,
    };
    insert_socket(socket)
}

/// Bind a socket to a local and remote address. If one of these should remain
/// unbound, such as a socket that only accepts incoming traffic, set the
/// address and port to all zeroes.
pub fn bind_socket(socket: SocketHandle, local_ip: IPV4Address, local_port: SocketPort, remote_ip: IPV4Address, remote_port: SocketPort) -> Result<(), NetError> {
    if let Some(sock) = OPEN_SOCKETS.write().get_mut(&socket) {
        sock.bind(local_ip, local_port, remote_ip, remote_port);
    } else {
        return Err(NetError::InvalidSocket);
    }

    let mut sockets_by_port = SOCKETS_BY_PORT.write();
    if sockets_by_port.contains_key(&local_port) {
        return Err(NetError::PortAlreadyInUse);
    }
    sockets_by_port.insert(local_port, socket);
    Ok(())
}

fn socket_send_inner(dest_mac: [u8; 6], packet: Vec<u8>) -> Result<(), NetError> {
    let (source_mac, device_name) = with_active_device(|netdev| (netdev.mac, netdev.device_name.clone()))
        .map_err(|_| NetError::NoNetDevice)?;

    let mut total_frame = Vec::with_capacity(EthernetFrame::get_size() + packet.len());
    let eth_header = EthernetFrame::new_ipv4(source_mac, dest_mac);
    total_frame.extend_from_slice(eth_header.as_buffer());
    total_frame.extend(packet);

    let dev = open_path(&device_name).map_err(|_| NetError::DeviceDriverError)?;
    write_file(dev, &total_frame).map_err(|_| NetError::DeviceDriverError)?;
    close_file(dev).map_err(|_| NetError::DeviceDriverError)?;
    Ok(())
}

pub fn socket_broadcast(socket: SocketHandle, payload: &[u8]) -> Result<(), NetError> {
    let packet = match OPEN_SOCKETS.read().get(&socket) {
        Some(sock) => sock.create_packet(payload),
        None => return Err(NetError::InvalidSocket),
    };
    let dest_mac: [u8; 6] = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff];

    socket_send_inner(dest_mac, packet)
}

pub fn socket_send(socket: SocketHandle, payload: &[u8]) -> Result<(), NetError> {
    let (dest_ip, packet) = match OPEN_SOCKETS.read().get(&socket) {
        Some(sock) => (sock.binding.remote_ip, sock.create_packet(payload)),
        None => return Err(NetError::InvalidSocket),
    };

    if *dest_ip == [0, 0, 0, 0] {
        return Err(NetError::UnboundSocket);
    }

    let dest_mac = resolve_mac_from_ip(dest_ip)?;

    socket_send_inner(dest_mac, packet)
}

pub fn handle_incoming_tcp(remote_ip: IPV4Address, local_ip: IPV4Address, packet: &[u8]) -> Result<(), NetError> {
    let tcp_header = TCPHeader::from_buffer(packet).ok_or(NetError::IncompletePacket)?;
    crate::kprintln!("TCP Packet to port {}", tcp_header.get_destination_port());
    // first, check if the local port is listening to incoming traffic
    let listener_handle = get_socket_on_port(tcp_header.get_destination_port()).ok_or(NetError::PortNotOpen)?;
    // and confirm that it's a TCP socket
    match OPEN_SOCKETS.read().get(&listener_handle) {
        Some(sock) => {
            if let SocketProtocol::UDP = sock.protocol {
                return Err(NetError::WrongProtocol);
            }
        },
        None => return Err(NetError::InvalidSocket),
    }
    // check if a connection to that remote endpoint is already established
    let conn_handle = match get_tcp_connection_socket(local_ip, tcp_header.get_destination_port(), remote_ip, tcp_header.get_source_port()) {
        Some(handle) => handle,
        None => {
            // If no connection exists yet, 
            if !tcp_header.is_syn() {
                return Err(NetError::PortNotOpen);
            }
            // establish a connection
            add_pending_connection(
                remote_ip,
                tcp_header.get_source_port(),
                local_ip,
                tcp_header.get_destination_port(),
                tcp_header.sequence_number.to_be(),
            );
            crate::kprintln!("Add pending connection from {} {}", remote_ip, tcp_header.get_source_port());
            return Ok(());
        },
    };

    crate::kprintln!("Found the connection");

    let response = {
        let mut sockets = OPEN_SOCKETS.write();
        let conn_socket = sockets.get_mut(&conn_handle).ok_or(NetError::InvalidSocket)?;
        let connection = conn_socket.tcp_connection.as_mut().expect("Conn doesn't have connection object");
        let action = action_for_tcp_packet(connection, tcp_header);

        match action {
            TCPAction::Close | TCPAction::Reset => {
                None
            },
            TCPAction::Enqueue => {
                None
            },
            TCPAction::Discard => {
                None
            },
            TCPAction::Connect => {
                connection.state = TCPState::Established;
                crate::kprintln!("Full duplex established");
                // No need to acknowledge an ACK
                None
            },
        }
    };
    if let Some(packet) = response {
        let dest_mac = resolve_mac_from_ip(remote_ip)?;
        socket_send_inner(dest_mac, packet)
    } else {
        Ok(())
    }
}

pub fn socket_accept(handle: SocketHandle) -> Option<SocketHandle> {
    let port = OPEN_SOCKETS.read().get(&handle)?.binding.local_port;
    if port == SocketPort::new(0) {
        return None;
    }

    let pending = accept_pending_connection(port, None)?;
    let connection = TCPConnection::new(pending.seq_received);
    let initial_seq = connection.last_sequence_sent;
    let mut new_socket = OpenSocket::new_tcp();
    new_socket.bind(pending.local_ip, pending.local_port, pending.remote_ip, pending.remote_port);
    new_socket.set_tcp_connection(connection);
    let handle = insert_socket(new_socket);
    add_tcp_connection_lookup(pending.local_ip, pending.local_port, pending.remote_ip, pending.remote_port, handle);

    let packet = create_syn_ack(
        pending.local_ip,
        pending.local_port,
        pending.remote_ip,
        pending.remote_port,
        initial_seq,
        pending.seq_received + 1,
    );
    let dest_mac = resolve_mac_from_ip(pending.remote_ip).ok()?;
    socket_send_inner(dest_mac, packet);

    loop {
        match OPEN_SOCKETS.read().get(&handle) {
            Some(sock) => {
                if sock.is_connected() {
                    break;
                }
            },
            None => return None,
        }
        crate::task::actions::yield_coop();
    }

    Some(handle)
}

