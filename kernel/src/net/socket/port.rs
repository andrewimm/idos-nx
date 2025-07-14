#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
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

impl core::fmt::Display for SocketPort {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!(":{}", self.0))
    }
}
