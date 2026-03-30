use core::sync::atomic::Ordering;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use idos_api::io::error::{IoError, IoResult};

use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::virt::scratch::UnmappedPage;
use crate::task::map::get_task;
use crate::task::paging::get_current_physical_address;

use super::{
    super::protocol::{
        ipv4::Ipv4Address,
        tcp::{connection::TcpConnection, header::TcpHeader},
        udp::create_datagram,
    },
    super::resident::net_respond,
    port::SocketPort,
    AsyncCallback, SocketId, SocketType,
};

struct UdpPendingRead {
    buffer_paddr: PhysicalAddress,
    buffer_len: usize,
    callback: AsyncCallback,
}

pub struct UdpListener {
    port: SocketPort,
    pending_reads: VecDeque<UdpPendingRead>,
    buffered_datagrams: VecDeque<Vec<u8>>,
}

impl UdpListener {
    pub fn new(port: SocketPort) -> Self {
        Self {
            port,
            pending_reads: VecDeque::new(),
            buffered_datagrams: VecDeque::new(),
        }
    }

    /// Called when a UDP packet arrives on this port.
    /// The read buffer format is: [sender_ip: 4][sender_port: 2 BE][payload]
    pub fn handle_packet(&mut self, remote_addr: Ipv4Address, remote_port: u16, data: &[u8]) {
        let mut datagram = Vec::with_capacity(6 + data.len());
        datagram.extend_from_slice(&remote_addr.0);
        datagram.extend_from_slice(&remote_port.to_be_bytes());
        datagram.extend_from_slice(data);

        if let Some(read) = self.pending_reads.pop_front() {
            let written = deliver_to_buffer(read.buffer_paddr, read.buffer_len, &datagram);
            complete_op(read.callback, Ok(written as u32));
        } else {
            self.buffered_datagrams.push_back(datagram);
        }
    }

    /// Read the next datagram. Returns [sender_ip: 4][sender_port: 2 BE][payload].
    /// If no datagram is buffered, queues the read for async completion.
    pub fn read(&mut self, buffer: &mut [u8], callback: AsyncCallback) -> Option<IoResult> {
        if let Some(datagram) = self.buffered_datagrams.pop_front() {
            let copy_len = buffer.len().min(datagram.len());
            buffer[..copy_len].copy_from_slice(&datagram[..copy_len]);
            return Some(Ok(copy_len as u32));
        }
        let buffer_vaddr = VirtualAddress::new(buffer.as_ptr() as u32);
        let buffer_paddr = get_current_physical_address(buffer_vaddr).unwrap();
        self.pending_reads.push_back(UdpPendingRead {
            buffer_paddr,
            buffer_len: buffer.len(),
            callback,
        });
        None
    }

    /// Write a UDP datagram. The buffer format is:
    ///   [dest_ip: 4 bytes] [dest_port: 2 bytes, big-endian] [payload...]
    /// If the local IP is known, the datagram is sent immediately and the
    /// callback is completed synchronously. Otherwise, the write is queued
    /// in PENDING_UDP_WRITES and will be flushed when DHCP completes.
    pub fn write(&self, buffer: &[u8], local_ip: Option<Ipv4Address>, callback: AsyncCallback) -> Option<IoResult> {
        if buffer.len() < 6 {
            return Some(Err(IoError::InvalidArgument));
        }
        let dest_ip = Ipv4Address([buffer[0], buffer[1], buffer[2], buffer[3]]);
        let dest_port = u16::from_be_bytes([buffer[4], buffer[5]]);
        let payload = &buffer[6..];

        if let Some(src_ip) = local_ip {
            let datagram = create_datagram(src_ip, *self.port, dest_ip, dest_port, payload);
            net_respond(dest_ip, datagram);
            complete_op(callback, Ok(payload.len() as u32));
            Some(Ok(payload.len() as u32))
        } else {
            // Queue for later — DHCP hasn't resolved yet
            PENDING_UDP_WRITES.lock().push_back(PendingUdpWrite {
                source_port: self.port,
                dest_ip,
                dest_port,
                payload: Vec::from(payload),
                callback,
            });
            None
        }
    }
}

/// Copy data into a userspace buffer via its physical address.
/// Caps at page boundary to avoid cross-page faults.
fn deliver_to_buffer(buffer_paddr: PhysicalAddress, buffer_len: usize, data: &[u8]) -> usize {
    let buffer_offset = buffer_paddr.as_u32() & 0xfff;
    let mapping = UnmappedPage::map(buffer_paddr & 0xfffff000);
    let buffer_ptr = (mapping.virtual_address() + buffer_offset).as_ptr_mut::<u8>();
    let page_remaining = 0x1000 - buffer_offset as usize;
    let usable_len = buffer_len.min(page_remaining);
    let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr, usable_len) };
    let write_length = data.len().min(buffer.len());
    buffer[..write_length].copy_from_slice(&data[..write_length]);
    write_length
}

pub struct PendingUdpWrite {
    pub source_port: SocketPort,
    pub dest_ip: Ipv4Address,
    pub dest_port: u16,
    pub payload: Vec<u8>,
    pub callback: AsyncCallback,
}

// TODO: These are global singletons, which only works with a single network
// device. When multi-device support is added, the pending queue and resolved IP
// should be per-device.
static PENDING_UDP_WRITES: spin::Mutex<VecDeque<PendingUdpWrite>> =
    spin::Mutex::new(VecDeque::new());

static RESOLVED_LOCAL_IP: spin::RwLock<Option<Ipv4Address>> = spin::RwLock::new(None);

/// Returns the local IP if DHCP has completed.
pub fn get_resolved_local_ip() -> Option<Ipv4Address> {
    *RESOLVED_LOCAL_IP.read()
}

/// Called when DHCP completes and the local IP is known.
/// Stores the IP for future synchronous access, then flushes all pending UDP writes.
pub fn flush_pending_udp_writes(local_ip: Ipv4Address) {
    *RESOLVED_LOCAL_IP.write() = Some(local_ip);
    let mut pending = PENDING_UDP_WRITES.lock();
    while let Some(write) = pending.pop_front() {
        let datagram = create_datagram(
            local_ip,
            *write.source_port,
            write.dest_ip,
            write.dest_port,
            &write.payload,
        );
        net_respond(write.dest_ip, datagram);
        complete_op(write.callback, Ok(write.payload.len() as u32));
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

    pub fn all_connection_ids(&self) -> alloc::vec::Vec<SocketId> {
        self.lookup.values().copied().collect()
    }
}

pub struct TcpListener {
    local_port: SocketPort,
    pub connections: ListenerConnections,
    pending_syn: VecDeque<(Ipv4Address, Ipv4Address, SocketPort, u32)>, // (local_addr, remote_addr, remote_port, seq)
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
            Some(_) => panic!(),
            None => {
                if tcp_header.is_syn() {
                    // If the packet is a SYN, we queue it for later processing
                    if self.pending_accept.is_empty() {
                        self.pending_syn.push_back((local_addr, remote_addr, remote_port, u32::from_be(tcp_header.sequence_number)));
                    } else {
                        // If we have a pending accept, we can immediately process the SYN
                        let callback = self.pending_accept.pop_front().unwrap();
                        return Some(self.init_connection(
                            local_addr,
                            remote_addr,
                            remote_port,
                            u32::from_be(tcp_header.sequence_number),
                            callback,
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
        callback: AsyncCallback,
    ) -> (SocketId, TcpConnection) {
        let is_outbound = last_seq == 0;
        let socket_id = SocketId::new(super::NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst));
        let mut connection = TcpConnection::new(
            socket_id,
            self.local_port,
            remote_addr,
            remote_port,
            is_outbound,
            Some((callback, !is_outbound)),
        );
        connection.last_sequence_received = last_seq;
        self.connections.add(remote_addr, remote_port, socket_id);
        let flags = if is_outbound {
            TcpHeader::FLAG_SYN
        } else {
            TcpHeader::FLAG_SYN | TcpHeader::FLAG_ACK
        };
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
        net_respond(remote_addr, response);

        (socket_id, connection)
    }

    /// Accept a new connection on this TCP listener.
    /// If a SYN has already been queued, immediately begin the handshake
    /// and return the new connection. Otherwise, queue the accept callback
    /// for when a SYN arrives later.
    pub fn accept(&mut self, buffer: &mut [u8], callback: AsyncCallback) -> Option<(SocketId, TcpConnection)> {
        if self.pending_syn.is_empty() {
            self.pending_accept.push_back(callback);
            return None;
        }
        let (local_addr, remote_addr, remote_port, seq) = self.pending_syn.pop_front().unwrap();
        Some(self.init_connection(local_addr, remote_addr, remote_port, seq, callback))
    }
}

pub fn complete_op(callback: AsyncCallback, result: IoResult) {
    let (task_id, io_index, op_id) = callback;
    let task_lock = match get_task(task_id) {
        Some(lock) => lock,
        None => return,
    };
    let io_entry = task_lock.read().async_io_complete(io_index);
    if let Some(entry) = io_entry {
        entry.inner().async_complete(op_id, result);
    }
}
