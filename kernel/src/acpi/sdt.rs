use super::table::{ACPITable, TableHeader};
use crate::memory::address::PhysicalAddress;

#[repr(transparent)]
pub struct SDTHeader(TableHeader);

impl SDTHeader {
    pub fn at_address(address: PhysicalAddress) -> &'static Self {
        unsafe { &*(address.as_u32() as *const Self) }
    }

    pub fn get_sdt_list_start(&self) -> *const u32 {
        let first_pointer = self as *const Self as u32 + core::mem::size_of::<Self>() as u32;
        first_pointer as *const u32
    }

    pub fn iter(&self) -> SDTIterator {
        let header_address = self as *const Self as u32;
        let content_offset = core::mem::size_of::<TableHeader>() as u32;
        let root_address = (header_address + content_offset) as *const u32;
        let content_length = self.0.length - content_offset;
        let table_count = content_length / core::mem::size_of::<u32>() as u32;
        SDTIterator::new(root_address, table_count)
    }
}

impl ACPITable for SDTHeader {}

pub struct SDTIterator {
    root_address: *const u32,
    table_count: isize,
    current_index: isize,
}

impl SDTIterator {
    pub fn new(root_address: *const u32, table_count: u32) -> Self {
        Self {
            root_address,
            table_count: table_count as isize,
            current_index: 0,
        }
    }
}

impl Iterator for SDTIterator {
    type Item = &'static TableHeader;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index >= self.table_count {
            return None;
        }
        unsafe {
            let table_offset = self.root_address.offset(self.current_index);
            let table_address = core::ptr::read_volatile(table_offset);
            self.current_index += 1;
            Some(&*(table_address as *const TableHeader))
        }
    }
}
