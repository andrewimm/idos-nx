
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

#[derive(Copy, Clone, Eq)]
#[repr(transparent)]
pub struct VirtualAddress(u32);

impl VirtualAddress {
    pub const fn new(addr: u32) -> Self {
        Self(addr)
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }

    pub fn get_page_directory_index(&self) -> usize {
        self.0 as usize >> 22
    }

    pub fn get_page_table_index(&self) -> usize {
        (self.0 as usize >> 12) & 0x3ff
    }

    pub fn is_page_aligned(&self) -> bool {
        self.0 & 0xfff == 0
    }

    pub fn next_page_barrier(&self) -> Self {
        if self.0 & 0xfff == 0 {
            Self::new(self.0)
        } else {
            let next = self.0 + 0x1000;
            Self::new(next & 0xfffff000)
        }
    }

    pub fn prev_page_barrier(&self) -> Self {
        Self::new(self.0 & 0xfffff000)
    }
}

impl From<VirtualAddress> for u32 {
    fn from(addr: VirtualAddress) -> Self {
        let value: u32 = addr.as_u32();
        value
    }
}

impl PartialEq for VirtualAddress {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl PartialOrd for VirtualAddress {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl core::cmp::Ord for VirtualAddress {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl core::ops::Add<u32> for VirtualAddress {
    type Output = VirtualAddress;

    fn add(self, rhs: u32) -> Self::Output {
        let new_addr = self.0.wrapping_add(rhs);
        VirtualAddress::new(new_addr)
    }
}

impl core::fmt::Debug for VirtualAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "VirtualAddress({:#010X})", self.0)
    }
}

