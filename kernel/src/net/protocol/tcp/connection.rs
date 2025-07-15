use alloc::vec::Vec;
use idos_api::io::error::{IOError, IOResult};

use crate::net::resident::net_send;
use crate::net::socket::listen::complete_op;
use crate::net::socket::AsyncCallback;

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
}

pub struct TcpConnection {
    own_id: SocketId,
    local_port: SocketPort,
    remote_address: Ipv4Address,
    remote_port: SocketPort,
    state: TcpState,
    pub last_sequence_sent: u32,
    pub last_sequence_received: u32,
    on_connect: Option<AsyncCallback>,
}

impl TcpConnection {
    pub fn new(
        own_id: SocketId,
        local_port: SocketPort,
        remote_address: Ipv4Address,
        remote_port: SocketPort,
        is_outbound: bool,
        on_connect: Option<AsyncCallback>,
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
            TcpAction::Connect => {
                self.state = TcpState::Established;
                self.last_sequence_sent += 1;
                if let Some(callback) = self.on_connect.take() {
                    complete_op(callback, Ok(*self.own_id));
                }
                None
            }
            TcpAction::Discard => None,
            TcpAction::Enqueue => {
                // TODO: store the data vec

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
            net_send(remote_addr, packet);
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
                // TODO: how should this case be handled?
                TcpAction::Discard
            }
            TcpState::SynReceived => {
                if !header.is_ack() {
                    return TcpAction::Reset;
                }
                let ack = u32::from_be(header.ack_number);
                if ack != self.last_sequence_sent + 1 {
                    return TcpAction::Reset;
                }
                crate::kprintln!("REALLY CONNECTED NOW");
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
        Some(Err(IOError::Unknown))
    }
}
