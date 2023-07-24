use alloc::collections::{VecDeque, BTreeMap};
use alloc::vec::Vec;
use spin::RwLock;

use crate::net::ip::IPHeader;
use crate::time::system::Timestamp;

use super::ip::{IPV4Address, IPProtocolType, Checksum};
use super::packet::PacketHeader;
use super::socket::{SocketPort, SocketHandle};

pub const TCP_FLAG_CWR: u8 = 0x80;
pub const TCP_FLAG_ECE: u8 = 0x40;
pub const TCP_FLAG_URG: u8 = 0x20;
pub const TCP_FLAG_ACK: u8 = 0x10;
pub const TCP_FLAG_PSH: u8 = 0x08;
pub const TCP_FLAG_RST: u8 = 0x04;
pub const TCP_FLAG_SYN: u8 = 0x02;
pub const TCP_FLAG_FIN: u8 = 0x01;

#[repr(C, packed)]
pub struct TCPHeader {
    pub source_port: u16,
    pub dest_port: u16,
    pub sequence_number: u32,
    pub ack_number: u32,
    /// size of the header in 32-bit words
    pub data_offset: u8,
    pub flags: u8,
    pub window_size: u16,
    pub checksum: u16,
    pub urgent_pointer: u16,
}

impl TCPHeader {
    pub fn byte_size(&self) -> usize {
        (self.data_offset & 0xff) as usize * 32
    }

    pub fn get_destination_port(&self) -> SocketPort {
        SocketPort::new(self.dest_port.to_be())
    }

    pub fn get_source_port(&self) -> SocketPort {
        SocketPort::new(self.source_port.to_be())
    }

    pub fn is_syn(&self) -> bool {
        self.flags & TCP_FLAG_SYN != 0
    }
    
    pub fn is_ack(&self) -> bool {
        self.flags & TCP_FLAG_ACK != 0
    }

    pub fn new_synack(source_ip: IPV4Address, source_port: SocketPort, dest_ip: IPV4Address, dest_port: SocketPort, seq: u32, ack: u32) -> Vec<u8> {
        let total_size = Self::get_size() + IPHeader::get_size();
        let mut packet_vec = Vec::new();
        for _ in 0..total_size {
            packet_vec.push(0);
        }
        let packet_buffer = packet_vec.as_mut_slice();
        let mut tcp_header = TCPHeader {
            source_port: source_port.to_be(),
            dest_port: dest_port.to_be(),
            sequence_number: seq.to_be(),
            ack_number: ack.to_be(),
            data_offset: ((TCPHeader::get_size() / 4) as u8) << 4,
            flags: TCP_FLAG_SYN | TCP_FLAG_ACK,
            window_size: 0xffff,
            checksum: 0,
            urgent_pointer: 0,
        };
        tcp_header.checksum = tcp_header.compute_checksum(source_ip, dest_ip, &[]);
        let tcp_start = tcp_header.copy_to_buffer(packet_buffer);
        let tcp_size = TCPHeader::get_size() as u16;

        let ip_header = IPHeader::new_tcp(source_ip, dest_ip, tcp_size, 127);
        let ip_header_space = &mut packet_buffer[..tcp_start];
        let ip_start = ip_header.copy_to_buffer(ip_header_space);
        assert_eq!(ip_start, 0, "Should not have extra space in the packet buffer");

        packet_vec
    }

    pub fn compute_checksum(&self, source_ip: IPV4Address, dest_ip: IPV4Address, data: &[u8]) -> u16 {
        let mut data_length = data.len();
        if data_length & 1 != 0 {
            data_length += 1;
        }
        let checksum_header = super::udp::IPChecksumHeader {
            source_ip,
            dest_ip,
            zeroes: 0,
            protocol: IPProtocolType::TCP as u8,
            udp_length: ((TCPHeader::get_size() + data_length) as u16).to_be(),
        };
        let mut checksum = Checksum::new();
        for value in checksum_header.as_u16_buffer().iter() {
            checksum.add_u16(*value);
        }

        let header_slice = unsafe {
            let ptr = self as *const TCPHeader as *const u16;
            let len = TCPHeader::get_size() / 2;
            core::slice::from_raw_parts(ptr, len)
        };
        for value in header_slice.iter() {
            checksum.add_u16(*value);
        }

        let mut i = 0;
        while i < data.len() {
            let low = data[i];
            let high = if i + 1 >= data.len() {
                0
            } else {
                data[i + 1]
            };
            let value = (low as u16) | ((high as u16) << 8);
            checksum.add_u16(value);
            i += 2;
        }

        checksum.compute()
    }
}

impl PacketHeader for TCPHeader {}

#[derive(Copy, Clone)]
pub enum TCPState {
    /// Client has initiated connection, waiting on server
    SynSent,
    /// Server has received a connection, waiting on client
    SynAckSent,
    Established,

    // need to handle closing
}

#[derive(Copy, Clone)]
pub struct TCPConnection {
    pub state: TCPState,
    pub last_sequence_sent: u32,
    pub last_sequence_received: u32,
}

impl TCPConnection {
    pub fn new(seq_received: u32) -> Self {
        let generated_sequence = 10;
        Self {
            state: TCPState::SynAckSent,
            last_sequence_sent: generated_sequence,
            last_sequence_received: seq_received,
        }
    }

    pub fn is_connected(&self) -> bool {
        match self.state {
            TCPState::Established => true,
            _ => false,
        }
    }
}

#[derive(Copy, Clone)]
pub struct PendingConnection {
    established: Timestamp,
    pub seq_received: u32,

    pub remote_ip: IPV4Address,
    pub remote_port: SocketPort,
    pub local_ip: IPV4Address,
    pub local_port: SocketPort,
}

static PENDING_CONNECTIONS: RwLock<BTreeMap<SocketPort, VecDeque<PendingConnection>>> = RwLock::new(BTreeMap::new());

pub fn add_pending_connection(remote_ip: IPV4Address, remote_port: SocketPort, local_ip: IPV4Address, local_port: SocketPort, seq_received: u32) {
    let established = crate::time::system::get_system_time().to_timestamp();

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

pub fn accept_pending_connection(port: SocketPort, timeout: Option<u32>) -> Option<PendingConnection> {
    let mut connections = PENDING_CONNECTIONS.write();
    let list = connections.get_mut(&port)?;
    list.pop_front()
}

pub struct TCPConnectionLookup {
    remote_ip: IPV4Address,
    remote_port: SocketPort,
    local_ip: IPV4Address,
    local_port: SocketPort,
    handle: SocketHandle,
}

static TCP_CONNECTIONS: RwLock<BTreeMap<SocketPort, Vec<TCPConnectionLookup>>> = RwLock::new(BTreeMap::new());

pub fn get_tcp_connection(local_ip: IPV4Address, local_port: SocketPort, remote_ip: IPV4Address, remote_port: SocketPort) -> Option<SocketHandle> {
    let connections = TCP_CONNECTIONS.read();
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

pub fn add_tcp_connection(local_ip: IPV4Address, local_port: SocketPort, remote_ip: IPV4Address, remote_port: SocketPort, handle: SocketHandle) {
    let lookup = TCPConnectionLookup {
        remote_ip,
        remote_port,
        local_ip,
        local_port,
        handle,
    };
    let mut connections = TCP_CONNECTIONS.write();
    if connections.get(&local_port).is_none() {
        let mut list = Vec::with_capacity(1);
        list.push(lookup);
        connections.insert(local_port, list);
        return;
    }
    connections.get_mut(&local_port).unwrap().push(lookup);
}

