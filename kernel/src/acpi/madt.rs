//! Multiple APIC Description Table (MADT)
//! This is where the information on CPU cores, APICs and IOAPICs is stored.
//! Using this table, the kernel is able to run in a multi-core environment.

use super::table::{ACPITable, TableHeader};

#[repr(C, packed)]
pub struct MADT {
    header: TableHeader,
    local_apic_address: u32,
    flags: u32,
}

impl MADT {
    pub fn at_address(address: u32) -> &'static Self {
        unsafe { &*(address as *const Self) }
    }

    pub fn iter(&self) -> impl Iterator<Item = &'static MADTEntry> {
        MADTIterator::new(self as *const Self as *const u8)
    }
}

impl ACPITable for MADT {}

pub struct MADTIterator {
    table_root: *const u8,
    read_offset: isize,
    max_offset: isize,
}

impl MADTIterator {
    pub fn new(table_root: *const u8) -> Self {
        let header = unsafe { &*(table_root as *const TableHeader) };
        Self {
            table_root,
            read_offset: core::mem::size_of::<MADT>() as isize,
            max_offset: header.length as isize,
        }
    }
}

impl Iterator for MADTIterator {
    type Item = &'static MADTEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.read_offset >= self.max_offset {
            return None;
        }
        let entry_ptr = unsafe { self.table_root.offset(self.read_offset) as *const MADTEntry };
        let entry = unsafe { &*entry_ptr };
        crate::kprintln!("ENTRY {} {}", entry.entry_type, entry.length);
        self.read_offset += entry.length as isize;
        Some(entry)
    }
}

#[repr(C, packed)]
pub struct MADTEntry {
    entry_type: u8,
    length: u8,
}

#[derive(Debug)]
#[repr(C, packed)]
pub struct MADTLocalAPIC {
    pub entry_type: u8,
    pub length: u8,
    pub processor_id: u8,
    pub apic_id: u8,
    pub flags: u32,
}

#[derive(Debug)]
#[repr(C, packed)]
pub struct MADTIOAPIC {
    pub entry_type: u8,
    pub length: u8,
    pub ioapic_id: u8,
    pub _reserved: u8,
    pub ioapic_address: u32,
    pub global_system_interrupt_base: u32,
}

#[derive(Debug)]
#[repr(C, packed)]
pub struct InterruptSourceOverride {
    pub entry_type: u8,
    pub length: u8,
    pub bus_source: u8,
    pub irq_source: u8,
    pub global_system_interrupt: u32,
    pub flags: u16,
}

pub enum MADTEntryType {
    LocalAPIC(&'static MADTLocalAPIC),
    IOAPIC(&'static MADTIOAPIC),
    InterruptSourceOverride(&'static InterruptSourceOverride),
    IONMI,
    LocalNMI,
    Unknown,
}

impl MADTEntry {
    pub fn next_entry(&self) -> *const MADTEntry {
        unsafe { &*((self as *const _ as u32 + self.length as u32) as *const MADTEntry) }
    }

    pub fn refine(&self) -> MADTEntryType {
        match self.entry_type {
            0 => MADTEntryType::LocalAPIC(unsafe { &*(self as *const _ as *const MADTLocalAPIC) }),
            1 => MADTEntryType::IOAPIC(unsafe { &*(self as *const _ as *const MADTIOAPIC) }),
            2 => MADTEntryType::InterruptSourceOverride(unsafe {
                &*(self as *const _ as *const InterruptSourceOverride)
            }),
            3 => MADTEntryType::IONMI,
            4 => MADTEntryType::LocalNMI,
            _ => MADTEntryType::Unknown,
        }
    }
}
