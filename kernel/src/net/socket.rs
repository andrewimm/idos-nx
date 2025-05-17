use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::ops::Deref;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use crate::task::actions::handle::create_file_handle;
use crate::task::actions::io::{close_sync, open_sync, write_sync};

use super::arp::{add_network_translation, resolve_mac_from_ip};
use super::error::NetError;
use super::ethernet::{EthernetFrameHeader, HardwareAddress};
use super::ip::{IPProtocolType, IPV4Address, IPV4Header};
use super::packet::PacketHeader;
use super::tcp::connection::{
    action_for_tcp_packet, add_tcp_connection_lookup, get_tcp_connection_socket,
    remove_tcp_connection_lookup, TCPAction, TCPConnection, TCPState,
};
use super::tcp::header::{create_tcp_packet, TCPHeader, TCP_FLAG_ACK, TCP_FLAG_FIN, TCP_FLAG_SYN};
use super::tcp::pending::{accept_pending_connection, add_pending_connection};
use super::tcp::queued::add_packet;
use super::tcp::queued::get_latest_packet;
use super::udp::{create_datagram, UDPHeader};
use super::with_active_device;

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

    pub fn bind(
        &mut self,
        local_ip: IPV4Address,
        local_port: SocketPort,
        remote_ip: IPV4Address,
        remote_port: SocketPort,
    ) {
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
            SocketProtocol::UDP => create_datagram(
                self.binding.local_ip,
                *self.binding.local_port,
                self.binding.remote_ip,
                *self.binding.remote_port,
                payload,
            ),
            SocketProtocol::TCP => {
                //create_tcp_packet(payload);
                panic!("TCP not working");
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SocketHandle(pub u32);

impl Deref for SocketHandle {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

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

pub fn receive_ip_packet(source_mac: HardwareAddress, raw: &[u8]) {
    let ip_header = match IPV4Header::try_from_u8_buffer(raw) {
        Some(header) => header,
        None => return,
    };
    let total_length = ip_header.total_length.to_be() as usize;
    let remainder = &raw[IPV4Header::get_size()..total_length];
    add_network_translation(ip_header.source, source_mac);
    if ip_header.protocol == IPProtocolType::TCP {
        handle_incoming_tcp(ip_header.source, ip_header.dest, remainder).unwrap();
    } else if ip_header.protocol == IPProtocolType::UDP {
        let udp_header = match UDPHeader::try_from_u8_buffer(remainder) {
            Some(header) => header,
            None => return,
        };
        let dest_port = udp_header.dest_port.to_be();

        if dest_port == 68 {}
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
pub fn bind_socket(
    socket: SocketHandle,
    local_ip: IPV4Address,
    local_port: SocketPort,
    remote_ip: IPV4Address,
    remote_port: SocketPort,
) -> Result<(), NetError> {
    if let Some(sock) = OPEN_SOCKETS.write().get_mut(&socket) {
        crate::kprintln!(
            "BIND SOCKET: {:}:{} {:}:{}",
            local_ip,
            local_port,
            remote_ip,
            remote_port
        );
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

fn socket_send_inner(dest_mac: HardwareAddress, packet: Vec<u8>) -> Result<(), NetError> {
    let (source_mac, device_name) =
        with_active_device(|netdev| (netdev.mac, netdev.device_name.clone()))
            .map_err(|_| NetError::NoNetDevice)?;

    let mut total_frame = Vec::with_capacity(EthernetFrameHeader::get_size() + packet.len());
    let eth_header = EthernetFrameHeader::new_ipv4(source_mac, dest_mac);
    total_frame.extend_from_slice(eth_header.as_u8_buffer());
    total_frame.extend(packet);

    let dev = create_file_handle();
    open_sync(dev, &device_name).map_err(|_| NetError::DeviceDriverError)?;
    write_sync(dev, &total_frame, 0).map_err(|_| NetError::DeviceDriverError)?;
    close_sync(dev).map_err(|_| NetError::DeviceDriverError)?;
    Ok(())
}

pub fn socket_broadcast(socket: SocketHandle, payload: &[u8]) -> Result<(), NetError> {
    let packet = match OPEN_SOCKETS.read().get(&socket) {
        Some(sock) => sock.create_packet(payload),
        None => return Err(NetError::InvalidSocket),
    };
    let dest_mac: HardwareAddress = HardwareAddress::broadcast();

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

pub fn handle_incoming_tcp(
    remote_ip: IPV4Address,
    local_ip: IPV4Address,
    packet: &[u8],
) -> Result<(), NetError> {
    let tcp_header = TCPHeader::try_from_u8_buffer(packet).ok_or(NetError::IncompletePacket)?;
    // first, check if the local port is listening to incoming traffic
    let listener_handle =
        get_socket_on_port(tcp_header.get_destination_port()).ok_or(NetError::PortNotOpen)?;
    // and confirm that it's a TCP socket
    match OPEN_SOCKETS.read().get(&listener_handle) {
        Some(sock) => {
            if let SocketProtocol::UDP = sock.protocol {
                return Err(NetError::WrongProtocol);
            }
        }
        None => return Err(NetError::InvalidSocket),
    }
    // check if a connection to that remote endpoint is already established
    let conn_handle = match get_tcp_connection_socket(
        tcp_header.get_destination_port(),
        remote_ip,
        tcp_header.get_source_port(),
    ) {
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
            return Ok(());
        }
    };

    let response = {
        let mut sockets = OPEN_SOCKETS.write();
        let conn_socket = sockets
            .get_mut(&conn_handle)
            .ok_or(NetError::InvalidSocket)?;
        let connection = conn_socket
            .tcp_connection
            .as_mut()
            .expect("Conn doesn't have connection object");
        let action = action_for_tcp_packet(connection, tcp_header);

        match action {
            TCPAction::Close => {
                sockets.remove(&conn_handle);
                remove_tcp_connection_lookup(
                    tcp_header.get_destination_port(),
                    remote_ip,
                    tcp_header.get_source_port(),
                );
                None
            }
            TCPAction::Reset => None,
            TCPAction::Enqueue => {
                //connection.last_sequence_received = tcp_header.sequence_number.to_be();
                //connection.last_sequence_sent += 1;

                let data_start = tcp_header.byte_size();
                let data_size = packet.len() - data_start;

                let mut data_vec = Vec::with_capacity(data_size);
                for _ in 0..data_size {
                    data_vec.push(0);
                }
                data_vec
                    .as_mut_slice()
                    .copy_from_slice(&packet[data_start..]);
                add_packet(conn_handle, data_vec);

                Some(create_tcp_packet(
                    conn_socket.binding.local_ip,
                    conn_socket.binding.local_port,
                    conn_socket.binding.remote_ip,
                    conn_socket.binding.remote_port,
                    connection.last_sequence_sent,
                    tcp_header.sequence_number.to_be() + data_size as u32,
                    TCP_FLAG_ACK,
                    &[],
                ))
            }
            TCPAction::Discard => None,
            TCPAction::Connect => {
                connection.state = TCPState::Established;
                connection.last_sequence_sent += 1;
                // No need to acknowledge an ACK
                None
            }
            TCPAction::FinAck => {
                connection.state = TCPState::LastAck;
                Some(create_tcp_packet(
                    conn_socket.binding.local_ip,
                    conn_socket.binding.local_port,
                    conn_socket.binding.remote_ip,
                    conn_socket.binding.remote_port,
                    connection.last_sequence_sent,
                    tcp_header.sequence_number.to_be() + 1,
                    TCP_FLAG_FIN | TCP_FLAG_ACK,
                    &[],
                ))
            }
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
    let mut new_socket = OpenSocket::new_tcp();
    new_socket.bind(
        pending.local_ip,
        pending.local_port,
        pending.remote_ip,
        pending.remote_port,
    );
    new_socket.set_tcp_connection(connection);
    let handle = insert_socket(new_socket);
    add_tcp_connection_lookup(
        pending.local_port,
        pending.remote_ip,
        pending.remote_port,
        handle,
    );

    let packet = create_tcp_packet(
        pending.local_ip,
        pending.local_port,
        pending.remote_ip,
        pending.remote_port,
        connection.last_sequence_sent,
        pending.seq_received + 1,
        TCP_FLAG_SYN | TCP_FLAG_ACK,
        &[],
    );
    let dest_mac = resolve_mac_from_ip(pending.remote_ip).ok()?;
    socket_send_inner(dest_mac, packet).unwrap();

    loop {
        match OPEN_SOCKETS.read().get(&handle) {
            Some(sock) => {
                if sock.is_connected() {
                    break;
                }
            }
            None => return None,
        }
        crate::task::actions::yield_coop();
    }

    Some(handle)
}

pub fn socket_read(handle: SocketHandle, buffer: &mut [u8]) -> Option<usize> {
    let payload = get_latest_packet(handle)?;
    let length = buffer.len().min(payload.len());
    buffer[..length].copy_from_slice(&payload[..length]);
    Some(length)
}
