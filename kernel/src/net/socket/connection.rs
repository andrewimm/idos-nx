pub struct ConnectionId(u32);

impl ConnectionId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl core::ops::Deref for ConnectionId {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct Connection {}
