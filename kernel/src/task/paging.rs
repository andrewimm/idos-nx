use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::physical::allocate_frame;
use crate::memory::physical::allocated_frame::AllocatedFrame;
use crate::memory::virt::invalidate_page;
use crate::memory::virt::page_table::PageTable;
use crate::memory::virt::scratch::UnmappedPage;
use super::id::TaskID;
use super::memory::{MemMappedRegion, MemoryBacking};
use super::switching::{get_current_task, get_task};

/// PermissionFlags are used by kernel or user code to request the extra
/// permission bits applied to paged memory
pub struct PermissionFlags(u8);

impl PermissionFlags {
    pub const USER_ACCESS: u8 = 1;
    pub const WRITE_ACCESS: u8 = 2;
    pub const NO_RECLAIM: u8 = 4;

    pub fn new(flags: u8) -> PermissionFlags {
        PermissionFlags(flags)
    }

    pub fn empty() -> PermissionFlags {
        PermissionFlags(0)
    }

    pub fn as_u8(&self) -> u8 {
        self.0
    }

    /// Check if the set of permission flags contains a specific flag.
    /// If more than one flag is passed, returns true if any of the flags are
    /// set.
    pub fn has_flag(&self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

/// Assuming a virtual address is not currently backed by a physical frame,
/// identify if it is part of the current task's memory map, and add an
/// appropriate frame to the current page table.
pub fn page_on_demand(address: VirtualAddress) -> Option<PhysicalAddress> {
    let task_lock = get_current_task();
    let exec_segment = task_lock
        .read()
        .memory_mapping
        .get_execution_segment_containing_address(&address)
        .cloned();

    if let Some(segment) = exec_segment {
        let allocated_frame = allocate_frame().ok()?;
        let flags = PermissionFlags::new(PermissionFlags::USER_ACCESS);
        let page_start = address.prev_page_barrier();
        let paddr = current_pagedir_map(allocated_frame, page_start, flags);

        let current_exec = task_lock.read().current_executable.clone();
        if let Some(exec) = current_exec {
            segment.fill_frame(exec, page_start);
        }
        return Some(paddr);
    }

    {
        let task = task_lock.read();
        if let Some(mapping) = task.memory_mapping.get_mapping_containing_address(&address) {
            let allocated_frame = get_frame_for_region(mapping)?;
            let flags = get_flags_for_region(mapping);
            return Some(current_pagedir_map(allocated_frame, address.prev_page_barrier(), flags));
        }
    }

    None
}

/// Create a new page directory, copying the kernel-space entries from the
/// current one. All page directories share kernel-space mappings.
pub fn create_page_directory() -> PhysicalAddress {
    let addr = allocate_frame().unwrap().to_physical_address();
    // map the pagedir to a scratch page, and copy contents from the kernel
    // space of the current pagedir
    {
        let unmapped = UnmappedPage::map(addr);
        let current_dir = PageTable::at_address(VirtualAddress::new(0xfffff000));
        let new_dir = PageTable::at_address(unmapped.virtual_address());
        for i in 0x300..0x400 {
            *(new_dir.get_mut(i)) = *(current_dir.get(i));
        }
        // Maintain the self-mapping property of the topmost entry!
        new_dir.get_mut(0x3ff).set_address(addr);
    }
    addr
}

/// Map a physical frame to the specified virtual address, in the current page
/// directory. Returns the physical address of the frame, in case that's
/// useful to the caller.
pub fn current_pagedir_map(frame: AllocatedFrame, vaddr: VirtualAddress, flags: PermissionFlags) -> PhysicalAddress {
    let paddr = frame.to_physical_address();
    current_pagedir_map_explicit(paddr, vaddr, flags);
    paddr
}

/// Actual implementation of mapping a page in virtual memory.
/// Modifies the current pagedir, adding a table frame if necessary.
pub fn current_pagedir_map_explicit(paddr: PhysicalAddress, vaddr: VirtualAddress, flags: PermissionFlags) {
    crate::kprint!("Mapping {:?} to {:?}\n", vaddr, paddr);
    let current_dir = PageTable::at_address(VirtualAddress::new(0xfffff000));
    let dir_index = vaddr.get_page_directory_index();
    let table_index = vaddr.get_page_table_index();

    let entry = current_dir.get_mut(dir_index);
    let table_address = VirtualAddress::new(0xffc00000 + (dir_index as u32 * 0x1000));
    let mut needs_invalidation = false;
    if !entry.is_present() {
        let frame_addr = allocate_frame().unwrap().to_physical_address();
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
    if flags.has_flag(PermissionFlags::USER_ACCESS) {
        table.get_mut(table_index).set_user_access();
    }
    if flags.has_flag(PermissionFlags::WRITE_ACCESS) {
        table.get_mut(table_index).set_write_access();
    }
    if flags.has_flag(PermissionFlags::NO_RECLAIM) {
        table.get_mut(table_index).set_no_reclaim();
    }

    if needs_invalidation {
        invalidate_page(vaddr);
    }
}

pub fn current_pagedir_unmap(vaddr: VirtualAddress) -> Option<PhysicalAddress> {
    crate::kprint!("Unmapping {:?}\n", vaddr);
    let current_dir = PageTable::at_address(VirtualAddress::new(0xfffff000));
    let dir_index = vaddr.get_page_directory_index();
    let table_index = vaddr.get_page_table_index();

    let entry = current_dir.get(dir_index);
    if !entry.is_present() {
        return None;
    }
    let table_address = VirtualAddress::new(0xffc00000 + (dir_index as u32 * 0x1000));
    let table = PageTable::at_address(table_address);
    if !table.get(table_index).is_present() {
        return None;
    }
    table.get_mut(table_index).clear_present();
    Some(table.get(table_index).get_address())
}

/// Get the physical address backing a virtual address in the current page
/// directory.
/// If that part of memory is not backed by anything, this method returns None.
pub fn get_current_physical_address(vaddr: VirtualAddress) -> Option<PhysicalAddress> {
    let offset = vaddr.as_u32() & 0xfff;

    let dir_index = vaddr.get_page_directory_index();
    let table_index = vaddr.get_page_table_index();
    let current_dir = PageTable::at_address(VirtualAddress::new(0xfffff000));
    let dir_entry = current_dir.get(dir_index);
    if !dir_entry.is_present() {
        return None;
    }
    let table_address = VirtualAddress::new(0xffc00000 + (dir_index as u32 * 0x1000));
    let table = PageTable::at_address(table_address);
    let table_entry = table.get(table_index);
    if !table_entry.is_present() {
        return None;
    }

    Some(table_entry.get_address() + offset)
}

pub fn get_frame_for_region(region: &MemMappedRegion) -> Option<AllocatedFrame> {
    match region.backed_by {
        MemoryBacking::Anonymous => allocate_frame().ok(),
        // TODO: needs a way to guarantee <16MiB
        MemoryBacking::DMA => allocate_frame().ok(),
        MemoryBacking::Direct(paddr) => Some(AllocatedFrame::new(paddr)),
        _ => panic!("Unsupported physical backing"),
    }
}

pub fn get_flags_for_region(region: &MemMappedRegion) -> PermissionFlags {
    let mut flags = PermissionFlags::USER_ACCESS | PermissionFlags::WRITE_ACCESS;

    // Physical memory explicitly backing a region should not be freed when a
    // page is cleaned up
    if let MemoryBacking::Direct(_) = region.backed_by {
        flags |= PermissionFlags::NO_RECLAIM;
    }

    PermissionFlags::new(flags)
}

/// Represents the page directory of a separate task
pub struct ExternalPageDirectory {
    id: TaskID,
    page_directory_location: PhysicalAddress,
}

impl ExternalPageDirectory {
    pub fn for_task(id: TaskID) -> Self {
        let task_lock = get_task(id).unwrap();
        let page_directory_location = task_lock.read().page_directory;

        Self {
            id,
            page_directory_location,
        }
    }

    pub fn unmap(&self, address: VirtualAddress) -> Option<PhysicalAddress> {
        crate::kprint!("Unmap {:?} for {:?}\n", address, self.id);
        let dir_index = address.get_page_directory_index();
        let table_index = address.get_page_table_index();

        let unmapped_for_dir = UnmappedPage::map(self.page_directory_location);
        let page_dir = PageTable::at_address(unmapped_for_dir.virtual_address());
        if !page_dir.get(dir_index).is_present() {
            return None;
        }
        let table_location = page_dir.get(dir_index).get_address();

        let unmapped_for_table = UnmappedPage::map(table_location);
        let page_table = PageTable::at_address(unmapped_for_table.virtual_address());
        if !page_table.get(table_index).is_present() {
            return None;
        }
        let backing = page_table.get(table_index).get_address();
        page_table.get_mut(table_index).clear_present();
        Some(backing)
    }
}
