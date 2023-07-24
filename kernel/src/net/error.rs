#[derive(Debug)]
pub enum NetError {
    NoNetDevice,
    DeviceDriverError,

    InvalidChecksum,
    IncompletePacket,

    /// Used a socket handle that does not exist
    InvalidSocket,
    /// Local port is already bound to another socket
    PortAlreadyInUse,
    /// Local port is not open for connections
    PortNotOpen,
    /// Attempted to send data to a socket with no remote endpoint
    UnboundSocket,
    /// Using a TCP packet as UDP, or vice-versa
    WrongProtocol,

    AddressNotResolved,
}
