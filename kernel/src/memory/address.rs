
#[derive(Copy, Clone, Eq)]
#[repr(transparent)]
pub struct PhysicalAddress(u32);

impl PhysicalAddress {
    pub const fn new(addr: u32) -> Self {
        Self(addr)
    }
}

impl Into<u32> for PhysicalAddress {
    fn into(self) -> u32 {
        self.0
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

impl core::fmt::Debug for PhysicalAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PhysicalAddress({:#010X})", self.0)
    }
}

