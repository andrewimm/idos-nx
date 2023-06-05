use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::physical::allocate_frame;
use crate::memory::physical::allocated_frame::AllocatedFrame;
use crate::memory::virt::invalidate_page;
use crate::memory::virt::page_table::PageTable;
use crate::memory::virt::scratch::UnmappedPage;
use super::memory::{MemMappedRegion, MemoryBacking};
use super::switching::get_current_task;

pub struct PermissionFlags(u8);

impl PermissionFlags {
    pub const USER_ACCESS: u8 = 1;
    pub const WRITE_ACCESS: u8 = 2;

    pub fn new(flags: u8) -> PermissionFlags {
        PermissionFlags(flags)
    }

    pub fn empty() -> PermissionFlags {
        PermissionFlags(0)
    }

    pub fn as_u8(&self) -> u8 {
        self.0
    }
}

pub fn page_on_demand(address: VirtualAddress) -> Option<PhysicalAddress> {
    let task_lock = get_current_task();
    let task = task_lock.read();
    let mapping = task.memory_mapping.get_mapping_containing_address(&address)?;
    let allocated_frame = get_frame_for_region(mapping)?;

    // TODO: set this from the mapping
    let flags = PermissionFlags::new(PermissionFlags::WRITE_ACCESS);

    Some(current_pagedir_map(allocated_frame, address.prev_page_barrier(), flags))
}

pub fn create_page_directory() -> PhysicalAddress {
    let addr = allocate_frame().unwrap().to_physical_address();
    // map the pagedir to a scratch page, and copy contents from the kernel
    // space of the current pagedir
    {
        let unmapped = UnmappedPage::map(addr);
        let current_dir = PageTable::at_address(VirtualAddress::new(0xfffff000));
        let new_dir = PageTable::at_address(unmapped.virtual_address());
        for i in 0..0x400 {
            *(new_dir.get_mut(i)) = *(current_dir.get(i));
        }
        new_dir.get_mut(0x3ff).set_address(addr);
    }
    addr
}

pub fn current_pagedir_map(frame: AllocatedFrame, vaddr: VirtualAddress, flags: PermissionFlags) -> PhysicalAddress {
    let paddr = frame.to_physical_address();
    current_pagedir_map_explicit(paddr, vaddr, flags);
    paddr
}

pub fn current_pagedir_map_explicit(paddr: PhysicalAddress, vaddr: VirtualAddress, flags: PermissionFlags) {
    crate::kprint!("Mapping {:?} to {:?}\n", vaddr, paddr);
    let current_dir = PageTable::at_address(VirtualAddress::new(0xfffff000));
    let dir_index = vaddr.get_page_directory_index();
    let table_index = vaddr.get_page_table_index();
    crate::kprint!("DIR: {}, TABLE {}\n", dir_index, table_index);

    let entry = current_dir.get_mut(dir_index);
    let table_address = VirtualAddress::new(0xffc00000 + (dir_index as u32 * 0x1000));
    let mut needs_invalidation = false;
    if !entry.is_present() {
        crate::kprint!("not present, add a table\n");
        let frame_addr = allocate_frame().unwrap().to_physical_address();
        crate::kprint!("frame addr {:?}\n", frame_addr);
        entry.set_address(frame_addr);
        entry.set_present();
        if dir_index < 768 {
            entry.set_user_access();
            entry.set_write_access();
        }
        let table = PageTable::at_address(table_address);
        table.zero();
    } else {
        let table = PageTable::at_address(table_address);
        needs_invalidation = table.get(table_index).is_present();
    }

    let table = PageTable::at_address(table_address);
    table.get_mut(table_index).set_address(paddr);
    table.get_mut(table_index).set_present();
    if flags.as_u8() & PermissionFlags::USER_ACCESS != 0 {
        table.get_mut(table_index).set_user_access();
    }
    if flags.as_u8() & PermissionFlags::WRITE_ACCESS != 0 {
        table.get_mut(table_index).set_write_access();
    }

    if needs_invalidation {
        invalidate_page(vaddr);
    }
}

pub fn get_frame_for_region(region: &MemMappedRegion) -> Option<AllocatedFrame> {
    match region.backed_by {
        MemoryBacking::Anonymous => allocate_frame().ok(),
        // TODO: needs a way to guarantee <16MiB
        MemoryBacking::DMA => allocate_frame().ok(),
        _ => panic!("Unsupported physical backing"),
    }
}

