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

pub struct TcpConnection {
    state: TcpState,
    pub last_sequence_sent: u32,
    pub last_sequence_received: u32,
}
