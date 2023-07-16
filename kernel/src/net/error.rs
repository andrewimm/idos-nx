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
}
