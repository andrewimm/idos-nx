
#[derive(Copy, Clone, Eq)]
#[repr(transparent)]
pub struct PhysicalAddress(u32);

impl PhysicalAddress {
    pub const fn new(addr: u32) -> Self {
        Self(addr)
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

impl From<PhysicalAddress> for u32 {
    fn from(addr: PhysicalAddress) -> Self {
        let value: u32 = addr.as_u32();
        value
    }
}

impl PartialEq for PhysicalAddress {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl PartialOrd for PhysicalAddress {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl core::cmp::Ord for PhysicalAddress {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl core::ops::Add<u32> for PhysicalAddress {
    type Output = PhysicalAddress;

    fn add(self, rhs: u32) -> Self::Output {
        let new_addr = self.0.wrapping_add(rhs);
        PhysicalAddress::new(new_addr)
    }
}

impl core::fmt::Debug for PhysicalAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PhysicalAddress({:#010X})", self.0)
    }
}

