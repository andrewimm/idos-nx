use core::{
    ops::{Add, Range, RangeInclusive, Sub},
    range::{Bound, RangeBounds},
};

/// Physical and Virtual addresses have a lot of common behaviors that should be
/// unified into a single trait, so that other types can depend on those features
pub trait MemoryAddress:
    Copy + Into<u32> + From<u32> + Add<u32, Output = Self> + Sub<u32, Output = Self>
{
}

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

impl MemoryAddress for PhysicalAddress {}

impl From<PhysicalAddress> for u32 {
    fn from(addr: PhysicalAddress) -> Self {
        let value: u32 = addr.as_u32();
        value
    }
}

impl From<u32> for PhysicalAddress {
    fn from(value: u32) -> Self {
        Self(value)
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

impl Add<u32> for PhysicalAddress {
    type Output = PhysicalAddress;

    fn add(self, rhs: u32) -> Self::Output {
        let new_addr = self.0.wrapping_add(rhs);
        Self::new(new_addr)
    }
}

impl Sub<u32> for PhysicalAddress {
    type Output = PhysicalAddress;

    fn sub(self, rhs: u32) -> Self::Output {
        let new_addr = self.0.saturating_sub(rhs);
        Self::new(new_addr)
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

    pub fn as_ptr<T>(&self) -> *const T {
        self.0 as usize as *const T
    }

    pub fn as_ptr_mut<T>(&self) -> *mut T {
        self.0 as usize as *mut T
    }
}

impl MemoryAddress for VirtualAddress {}

impl From<VirtualAddress> for u32 {
    fn from(addr: VirtualAddress) -> Self {
        let value: u32 = addr.as_u32();
        value
    }
}

impl From<u32> for VirtualAddress {
    fn from(value: u32) -> Self {
        Self(value)
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

impl Add<u32> for VirtualAddress {
    type Output = Self;

    fn add(self, rhs: u32) -> Self::Output {
        let new_addr = self.0.wrapping_add(rhs);
        Self::new(new_addr)
    }
}

impl Sub<VirtualAddress> for VirtualAddress {
    type Output = u32;

    fn sub(self, rhs: VirtualAddress) -> Self::Output {
        self.0.saturating_sub(rhs.0)
    }
}

impl Sub<u32> for VirtualAddress {
    type Output = Self;

    fn sub(self, rhs: u32) -> Self::Output {
        let new_addr = self.0.saturating_sub(rhs);
        Self::new(new_addr)
    }
}

impl core::fmt::Debug for VirtualAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "VirtualAddress({:#010X})", self.0)
    }
}

impl Default for VirtualAddress {
    fn default() -> Self {
        Self(0)
    }
}

pub trait AddressRange<T>: RangeBounds<T>
where
    T: MemoryAddress,
{
    fn get_first(&self) -> T {
        match self.start_bound() {
            Bound::Unbounded => T::from(0u32),
            Bound::Included(addr) => *addr,
            Bound::Excluded(addr) => *addr + 1,
        }
    }

    fn get_last(&self) -> T {
        match self.end_bound() {
            Bound::Unbounded => T::from(0xffffffffu32),
            Bound::Included(addr) => *addr,
            Bound::Excluded(addr) => *addr - 1,
        }
    }
}

impl<T> AddressRange<T> for Range<T> where T: MemoryAddress {}
impl<T> AddressRange<T> for RangeInclusive<T> where T: MemoryAddress {}
