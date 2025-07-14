use super::{super::protocol::ipv4::Ipv4Address, port::SocketPort};

pub struct SocketBinding {
    pub ip: Ipv4Address,
    pub port: SocketPort,
}

impl SocketBinding {
    pub fn any() -> Self {
        Self {
            ip: Ipv4Address([0, 0, 0, 0]),
            port: SocketPort::new(0),
        }
    }
}
