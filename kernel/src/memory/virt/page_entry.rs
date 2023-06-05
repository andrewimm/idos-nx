use super::super::address::PhysicalAddress;

pub const ENTRY_PRESENT: u32 = 1;
pub const ENTRY_WRITE_ACCESS: u32 = 1 << 1;
pub const ENTRY_USER_ACCESS: u32 = 1 << 2;
pub const ENTRY_WRITE_THROUGH: u32 = 1 << 3;
pub const ENTRY_CACHE_DISABLED: u32 = 1 << 4;
pub const ENTRY_ACCESSED: u32 = 1 << 5;
pub const ENTRY_DIRTY: u32 = 1 << 6;
pub const ENTRY_SIZE_EXTENDED: u32 = 1 << 7;
pub const ENTRY_GLOBAL: u32 = 1 << 8;

/// Represents an entry in a page table or directory. Entries are 32-bit
/// values with the following layout:
/// 31        11     9       0
/// | ADDRESS | FREE | FLAGS |
/// Bits 9, 10, 11 are unused and are available to the kernel for markers or
/// accounting.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PageTableEntry(u32);

impl PageTableEntry {
    pub const fn new() -> Self {
        Self(0)
    }

    pub fn zero(&mut self) {
        self.0 = 0;
    }

    pub fn get_address(&self) -> PhysicalAddress {
        PhysicalAddress::new(self.0 & 0xfffff000)
    }

    pub fn set_address(&mut self, addr: PhysicalAddress) {
        let addr_bits = addr.as_u32() & 0xfffff000;
        self.0 &= 0xfff;
        self.0 |= addr_bits;
    }

    pub fn set_present(&mut self) {
        self.0 |= ENTRY_PRESENT;
    }

    pub fn clear_present(&mut self) {
        self.0 &= !ENTRY_PRESENT;
    }

    pub fn is_present(&self) -> bool {
        self.0 & ENTRY_PRESENT != 0
    }

    pub fn set_user_access(&mut self) {
        self.0 |= ENTRY_USER_ACCESS;
    }

    pub fn set_write_access(&mut self) {
        self.0 |= ENTRY_WRITE_ACCESS;
    }
}
