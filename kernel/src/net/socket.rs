use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use crate::task::actions::io::{open_path, close_file, write_file};

use super::{dhcp::handle_incoming_packet, ip::{IPProtocolType, IPV4Address, IPHeader}, packet::PacketHeader, udp::{UDPHeader, create_datagram}, error::NetError, ethernet::EthernetFrame, with_active_device, arp::resolve_mac_from_ip};

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
}

pub enum SocketProtocol {
    UDP,
    TCP,
}

impl OpenSocket {
    pub fn bind(&mut self, local_ip: IPV4Address, local_port: SocketPort, remote_ip: IPV4Address, remote_port: SocketPort) {
        self.binding.local_ip = local_ip;
        self.binding.local_port = local_port;
        self.binding.remote_ip = remote_ip;
        self.binding.remote_port = remote_port;
    }

    pub fn create_packet(&self, payload: &[u8]) -> Vec<u8> {
        match self.protocol {
            SocketProtocol::UDP => {
                create_datagram(self.binding.local_ip, *self.binding.local_port, self.binding.remote_ip, *self.binding.remote_port, payload)
            },
            SocketProtocol::TCP => {
                panic!("TCP not supported");
            },
        }
    }
}

#[derive(Copy, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct SocketHandle(u32);

static OPEN_SOCKETS: RwLock<BTreeMap<SocketHandle, OpenSocket>> = RwLock::new(BTreeMap::new());
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
        crate::kprintln!("TCP not supported yet");
    } else if ip_header.protocol == IPProtocolType::UDP as u8 {
        let udp_header = match UDPHeader::from_buffer(remainder) {
            Some(header) => header,
            None => return,
        };
        let dest_port = udp_header.dest_port.to_be();
        crate::kprintln!("UDP Datagram to port {}", dest_port.clone());

        if dest_port == 68 {
            handle_incoming_packet(&remainder[UDPHeader::get_size()..]);
        }
    }
}

/// Create an unbound socket. A socket must be bound to a local or remote
/// address before it can be used for anything useful.
pub fn create_socket(protocol: SocketProtocol) -> SocketHandle {
    let socket = OpenSocket {
        binding: SocketBinding::empty(),
        protocol,
    };

    let handle = SocketHandle(NEXT_HANDLE.fetch_add(1, Ordering::SeqCst));

    OPEN_SOCKETS.write().insert(handle, socket);

    handle
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

