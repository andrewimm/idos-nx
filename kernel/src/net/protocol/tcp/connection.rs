use alloc::collections::VecDeque;
use alloc::vec::Vec;
use idos_api::io::error::{IOError, IOResult};

use crate::io::async_io::IOType;
use crate::io::provider::socket::SocketIOProvider;
use crate::io::provider::IOProvider;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::virt::scratch::UnmappedPage;
use crate::net::resident::net_respond;
use crate::net::socket::listen::complete_op;
use crate::net::socket::AsyncCallback;
use crate::task::map::get_task;
use crate::task::paging::get_current_physical_address;

use super::super::super::socket::{port::SocketPort, SocketId};
use super::super::{ipv4::Ipv4Address, packet::PacketHeader};
use super::header::TcpHeader;

#[derive(Clone, Copy)]
pub enum TcpState {
    /// Outbound connection is being established
    SynSent,
    /// Inbound connection is being established
    SynReceived,
    /// Connection is established and ready for data transfer
    Established,
    /// Received a FIN packet, waiting for ACK
    LastAck,
}

/// The TCPAction enum encodes a number of actions that should be performed
/// when a TCP packet is received. Because TCP is basically a state machine,
/// there are a number of cases where a received packet triggers some new
/// behavior beyond the core send/receive loop
pub enum TcpAction {
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
    ConnectAck,
}

struct PendingRead {
    buffer_paddr: PhysicalAddress,
    buffer_len: usize,
    callback: AsyncCallback,
}

pub struct TcpConnection {
    own_id: SocketId,
    local_port: SocketPort,
    remote_address: Ipv4Address,
    remote_port: SocketPort,
    state: TcpState,
    pub last_sequence_sent: u32,
    pub last_sequence_received: u32,

    on_connect: Option<(AsyncCallback, bool)>,
    pending_reads: VecDeque<PendingRead>,
    available_data: Vec<u8>,
}

impl TcpConnection {
    pub fn new(
        own_id: SocketId,
        local_port: SocketPort,
        remote_address: Ipv4Address,
        remote_port: SocketPort,
        is_outbound: bool,
        on_connect: Option<(AsyncCallback, bool)>,
    ) -> Self {
        Self {
            own_id,
            local_port,
            remote_address,
            remote_port,
            state: if is_outbound {
                TcpState::SynSent
            } else {
                TcpState::SynReceived
            },
            last_sequence_sent: 0,
            last_sequence_received: 0,
            on_connect,
            pending_reads: VecDeque::new(),
            available_data: Vec::new(),
        }
    }

    pub fn handle_packet(
        &mut self,
        local_addr: Ipv4Address,
        remote_addr: Ipv4Address,
        header: &TcpHeader,
        data: &[u8],
    ) {
        let action = self.action_for_tcp_packet(header);
        let packet_to_send = match action {
            TcpAction::Close => {
                // TODO: the socket connection needs to be cleaned up
                None
            }
            TcpAction::Connect | TcpAction::ConnectAck => {
                self.state = TcpState::Established;
                self.last_sequence_sent += 1;
                if let Some((callback, should_create_provider)) = self.on_connect.take() {
                    if should_create_provider {
                        let mut provider = SocketIOProvider::create_tcp();
                        provider.bind_to(*self.own_id);
                        let task_lock = match get_task(callback.0) {
                            Some(task) => task,
                            None => return,
                        };
                        let mut task_guard = task_lock.write();
                        let io_index = task_guard.async_io_table.add_io(IOType::Socket(provider));
                        let new_handle = task_guard.open_handles.insert(io_index);
                        drop(task_guard);
                        complete_op(callback, Ok(*new_handle as u32));
                    } else {
                        complete_op(callback, Ok(*self.own_id));
                    }
                }
                if let TcpAction::ConnectAck = action {
                    // If we established the connection, we need to send an ACK
                    Some(TcpHeader::create_packet(
                        local_addr,
                        self.local_port,
                        remote_addr,
                        self.remote_port,
                        self.last_sequence_sent,
                        u32::from_be(header.sequence_number) + 1,
                        TcpHeader::FLAG_ACK,
                        &[],
                    ))
                } else {
                    None
                }
            }
            TcpAction::Discard => None,
            TcpAction::Enqueue => {
                if self.pending_reads.is_empty() {
                    unimplemented!();
                } else {
                    // copy the buffer directly to the read buffer
                    let read = self.pending_reads.pop_front().unwrap();
                    let buffer_offset = read.buffer_paddr.as_u32() & 0xfff;
                    let mapping = UnmappedPage::map(read.buffer_paddr & 0xfffff000);
                    let buffer_ptr = (mapping.virtual_address() + buffer_offset).as_ptr_mut::<u8>();
                    let mut buffer =
                        unsafe { core::slice::from_raw_parts_mut(buffer_ptr, read.buffer_len) };
                    let write_length = data.len().min(buffer.len());

                    buffer[..write_length].copy_from_slice(&data[..write_length]);

                    // TODO: if we didn't write all the data, store the rest for the next read
                    complete_op(read.callback, Ok(write_length as u32));
                }

                Some(TcpHeader::create_packet(
                    local_addr,
                    self.local_port,
                    remote_addr,
                    self.remote_port,
                    self.last_sequence_sent,
                    u32::from_be(header.sequence_number) + data.len() as u32,
                    TcpHeader::FLAG_ACK,
                    &[],
                ))
            }
            TcpAction::FinAck => {
                self.state = TcpState::LastAck;
                Some(TcpHeader::create_packet(
                    local_addr,
                    self.local_port,
                    remote_addr,
                    self.remote_port,
                    self.last_sequence_sent,
                    u32::from_be(header.sequence_number) + 1,
                    TcpHeader::FLAG_FIN | TcpHeader::FLAG_ACK,
                    &[],
                ))
            }
            TcpAction::Reset => None,
        };

        if let Some(packet) = packet_to_send {
            net_respond(remote_addr, packet);
        }
    }

    /// Determine the action to take based on the current TCP state and the
    /// incoming packet.
    pub fn action_for_tcp_packet(&self, header: &TcpHeader) -> TcpAction {
        if header.is_rst() {
            // no matter what state the connection is in, a reset closes it
            return TcpAction::Close;
        }
        match self.state {
            TcpState::SynSent => {
                if !header.is_syn() || !header.is_ack() {
                    return TcpAction::Reset;
                }
                TcpAction::ConnectAck
            }
            TcpState::SynReceived => {
                if !header.is_ack() {
                    return TcpAction::Reset;
                }
                let ack = u32::from_be(header.ack_number);
                if ack != self.last_sequence_sent + 1 {
                    return TcpAction::Reset;
                }
                TcpAction::Connect
            }

            TcpState::Established => {
                if header.is_fin() {
                    return TcpAction::FinAck;
                }
                if header.is_syn() {
                    return TcpAction::Reset;
                }
                TcpAction::Enqueue
            }

            TcpState::LastAck => TcpAction::Close,
        }
    }

    pub fn read(&mut self, buffer: &mut [u8], callback: AsyncCallback) -> Option<IOResult> {
        if self.available_data.is_empty() {
            let buffer_vaddr = VirtualAddress::new(buffer.as_ptr() as u32);
            let buffer_paddr = get_current_physical_address(buffer_vaddr).unwrap();
            self.pending_reads.push_back(PendingRead {
                buffer_paddr,
                buffer_len: buffer.len(),
                callback,
            });
            return None;
        }
        Some(Err(IOError::Unknown))
    }
}
