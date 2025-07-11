/// Wrapper type for a 6-octet hardware MAC address.
/// This should be passed between methods of the network stack, rather than
/// a raw 6-byte array.
#[derive(Copy, Clone, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct HardwareAddress(pub [u8; 6]);

impl HardwareAddress {
    pub const BROADCAST: Self = HardwareAddress([0xff; 6]);

    /// shorthand for a MAC address that broadcasts to all devices (all octets
    /// set to 0xff)
    pub fn broadcast() -> Self {
        Self([0xff; 6])
    }
}

impl core::ops::Deref for HardwareAddress {
    type Target = [u8; 6];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for HardwareAddress {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl core::fmt::Display for HardwareAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(core::format_args!(
            "{}:{}:{}:{}:{}:{}",
            self[0],
            self[1],
            self[2],
            self[3],
            self[4],
            self[5]
        ))
    }
}
