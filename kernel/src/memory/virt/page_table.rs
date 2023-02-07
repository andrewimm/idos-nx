use core::arch::asm;
use super::page_entry::PageTableEntry;
use super::super::address::{PhysicalAddress, VirtualAddress};

pub const TABLE_ENTRY_COUNT: usize = 1024;

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PageTable([PageTableEntry; TABLE_ENTRY_COUNT]);

impl PageTable {
    pub fn at_address(addr: VirtualAddress) -> &'static mut PageTable {
        let ptr = addr.as_u32() as *mut PageTable;
        unsafe { &mut *ptr }
    }

    pub fn zero(&mut self) {
        for index in 0..1024 {
            self.0[index].zero();
        }
    }

    pub fn get(&self, index: usize) -> &PageTableEntry {
        &self.0[index & 0x3ff]
    }

    pub fn get_mut(&mut self, index: usize) -> &mut PageTableEntry {
        &mut self.0[index & 0x3ff]
    }
}

/// A reference to a valid page table located in physical memory, that can be
/// passed around and activated
#[derive(Copy, Clone)]
pub struct PageTableReference {
    address: PhysicalAddress,
}

impl PageTableReference {
    pub fn new(address: PhysicalAddress) -> Self {
        Self {
            address,
        }
    }

    pub fn make_active(&self) {
        set_current_pagedir(self.address);
    }
}

pub fn set_current_pagedir(phys: PhysicalAddress) {
    let addr = phys.as_u32();
    unsafe {
        asm!(
            "mov cr3, {addr:e}",
            addr = in(reg) addr,
        );
    }
}

pub fn get_current_pagedir() -> PhysicalAddress {
    let addr: u32;
    unsafe {
        asm!(
            "mov {addr:e}, cr3",
            addr = out(reg) addr,
        );
    }
    PhysicalAddress::new(addr)
}

