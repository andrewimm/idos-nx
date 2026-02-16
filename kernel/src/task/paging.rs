use idos_api::io::error::IoError;

use super::id::TaskID;
use super::map::get_task;
use super::memory::{get_file_backed_page, track_file_backed_page, MemMappedRegion, MemoryBacking};
use super::switching::get_current_task;
use crate::io::filesystem::driver_page_in_file;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::physical::allocated_frame::AllocatedFrame;
use crate::memory::physical::{
    allocate_frame, allocate_frame_with_tracking, allocate_frames, maybe_add_frame_reference,
    release_tracked_frame,
};
use crate::memory::virt::invalidate_page;
use crate::memory::virt::page_entry::PageTableEntry;
use crate::memory::virt::page_table::PageTable;
use crate::memory::virt::scratch::UnmappedPage;

/// PermissionFlags are used by kernel or user code to request the extra
/// permission bits applied to paged memory
#[derive(Copy, Clone)]
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

    let mem_mapping = task_lock
        .read()
        .memory_mapping
        .get_mapping_containing_address(&address)
        .cloned()?;

    // offset of the page within the mapping
    let page_offset = address.prev_page_barrier() - mem_mapping.address;
    // offset of the address within the page
    let local_offset = address.as_u32() & 0xfff;
    let flags = get_flags_for_region(&mem_mapping);

    let frame_start = match &mem_mapping.backed_by {
        MemoryBacking::IsaDma => {
            // DMA regions must be allocated as a contiguous block, so we
            // allocate the entire region at once
            let page_count = mem_mapping.page_count();
            let map_to = mem_mapping.address;
            let allocated_range =
                allocate_frames(page_count).expect("Failed to allocate DMA memory");
            let range_start = allocated_range.to_physical_address();

            for i in 0..page_count {
                let offset = i as u32 * 0x1000;
                current_pagedir_map_explicit(range_start + offset, map_to + offset, flags);
            }
            range_start + page_offset
        }
        MemoryBacking::Direct(_paddr) => {
            panic!("Shoudn't need to page Direct on demand, it's paged at map time");
        }
        MemoryBacking::FreeMemory => {
            // Free memory regions can be allocated on demand as needed, so we
            // allocate a single page for the requested address
            let allocated_frame =
                allocate_frame_with_tracking().expect("Failed to allocate memory for page");
            current_pagedir_map(allocated_frame, address.prev_page_barrier(), flags)
        }
        MemoryBacking::FileBacked {
            driver_id,
            mapping_token,
            offset_in_file,
            shared,
        } => {
            let total_offset = offset_in_file + page_offset;

            // Shared mappings reuse physical frames across tasks via the
            // tracker. Private mappings always get a fresh frame.
            if *shared {
                if let Some(paddr) = get_file_backed_page(*driver_id, *mapping_token, total_offset) {
                    super::LOGGER.log(format_args!("File-backed Mapping: re-use {:?}", paddr));
                    maybe_add_frame_reference(paddr);
                    current_pagedir_map(
                        AllocatedFrame::new(paddr),
                        address.prev_page_barrier(),
                        flags,
                    );
                    return Some(paddr);
                }
            }

            let allocated_frame =
                allocate_frame_with_tracking().expect("Failed to allocate memory");
            let frame_paddr = allocated_frame.peek_address();
            let result =
                match driver_page_in_file(*driver_id, *mapping_token, total_offset, frame_paddr) {
                    Some(immediate) => immediate,
                    None => {
                        task_lock.write().begin_file_mapping_request();
                        crate::task::actions::yield_coop();
                        let last_result = task_lock.write().last_map_result.take();
                        if let Some(result) = last_result {
                            result
                        } else {
                            Err(IoError::OperationFailed)
                        }
                    }
                };

            match result {
                Ok(_) => {
                    if *shared {
                        track_file_backed_page(*driver_id, *mapping_token, total_offset, frame_paddr);
                    }
                    current_pagedir_map(allocated_frame, address.prev_page_barrier(), flags)
                }
                Err(_) => {
                    let _ = release_tracked_frame(allocated_frame);
                    return None;
                }
            }
        }
    };

    Some(frame_start + local_offset)
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
        for i in 0..0x300 {
            *(new_dir.get_mut(i)) = PageTableEntry::new();
        }
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
pub fn current_pagedir_map(
    frame: AllocatedFrame,
    vaddr: VirtualAddress,
    flags: PermissionFlags,
) -> PhysicalAddress {
    let paddr = frame.to_physical_address();
    current_pagedir_map_explicit(paddr, vaddr, flags);
    paddr
}

/// Actual implementation of mapping a page in virtual memory.
/// Modifies the current pagedir, adding a table frame if necessary.
pub fn current_pagedir_map_explicit(
    paddr: PhysicalAddress,
    vaddr: VirtualAddress,
    flags: PermissionFlags,
) {
    super::LOGGER.log(format_args!("Pagedir: map {:?} to {:?}", vaddr, paddr));
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

pub fn current_pagedir_unmap(vaddr: VirtualAddress) -> Option<AllocatedFrame> {
    super::LOGGER.log(format_args!("Unmapping {:?}", vaddr));
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
    let paddr = table.get(table_index).get_address();
    Some(AllocatedFrame::new(paddr))
}

/// Get the physical address backing a virtual address in the current page
/// directory. If there is a valid mapping but the page has not been assigned
/// yet, it will be allocated and placed in the page table.
/// If there is no valid mapping for the given virtual address, returns None.
pub fn get_current_physical_address(vaddr: VirtualAddress) -> Option<PhysicalAddress> {
    if let Some(paddr) = maybe_get_current_physical_address(vaddr) {
        return Some(paddr);
    }
    page_on_demand(vaddr)
}

/// Similar to `get_current_physical_address(vaddr)`, but does not allocate
/// memory if the page table does not have an entry yet.
pub fn maybe_get_current_physical_address(vaddr: VirtualAddress) -> Option<PhysicalAddress> {
    let dir_index = vaddr.get_page_directory_index();
    let current_dir = PageTable::current_directory();
    let dir_entry = current_dir.get(dir_index);
    if !dir_entry.is_present() {
        return None;
    }
    let table_address = VirtualAddress::new(0xffc00000 + (dir_index as u32 * 0x1000));
    let table = PageTable::at_address(table_address);
    let table_index = vaddr.get_page_table_index();
    let table_entry = table.get(table_index);
    if !table_entry.is_present() {
        return None;
    }

    let offset = vaddr.as_u32() & 0xfff;
    Some(table_entry.get_address() + offset)
}

pub fn get_flags_for_region(region: &MemMappedRegion) -> PermissionFlags {
    let mut flags = PermissionFlags::USER_ACCESS | PermissionFlags::WRITE_ACCESS;

    // Physical memory explicitly backing a region should not be freed when a
    // page is cleaned up
    if let MemoryBacking::Direct(_) = region.backed_by {
        flags |= PermissionFlags::NO_RECLAIM;
    }

    // Shared file-backed mappings are read-only since multiple tasks share
    // the same physical frame
    if let MemoryBacking::FileBacked { shared: true, .. } = region.backed_by {
        flags &= !PermissionFlags::WRITE_ACCESS;
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

    pub fn map(&self, vaddr: VirtualAddress, paddr: PhysicalAddress, flags: PermissionFlags) {
        let dir_index = vaddr.get_page_directory_index();
        let table_index = vaddr.get_page_table_index();

        let (zero_frame, table_location) = {
            let unmapped_page_dir = UnmappedPage::map(self.page_directory_location);
            let page_dir = PageTable::at_address(unmapped_page_dir.virtual_address());
            let dir_entry = page_dir.get_mut(dir_index);
            let zero_frame = if !dir_entry.is_present() {
                let frame_addr = allocate_frame().unwrap().to_physical_address();
                dir_entry.set_address(frame_addr);
                dir_entry.set_present();
                if dir_index < 768 {
                    dir_entry.set_user_access();
                    dir_entry.set_write_access();
                }
                true
            } else {
                false
            };
            (zero_frame, dir_entry.get_address())
        };

        {
            let unmapped_page_table = UnmappedPage::map(table_location);
            let page_table = PageTable::at_address(unmapped_page_table.virtual_address());
            if zero_frame {
                // new page table should be zeroed out for safety
                page_table.zero();
            }
            let table_entry = page_table.get_mut(table_index);
            table_entry.set_address(paddr);
            table_entry.set_present();
            if flags.has_flag(PermissionFlags::USER_ACCESS) {
                table_entry.set_user_access();
            }
            if flags.has_flag(PermissionFlags::WRITE_ACCESS) {
                table_entry.set_write_access();
            }
            if flags.has_flag(PermissionFlags::NO_RECLAIM) {
                table_entry.set_no_reclaim();
            }
        }
    }

    pub fn unmap(&self, address: VirtualAddress) -> Option<AllocatedFrame> {
        super::LOGGER.log(format_args!("Unmapping {:?} for {:?}", address, self.id));
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
        let backing_frame = AllocatedFrame::new(backing);
        page_table.get_mut(table_index).clear_present();
        Some(backing_frame)
    }
}
