use idos_api::io::driver::DriverMappingToken;

use super::super::id::TaskID;
use super::super::map::get_task;
use super::super::memory::{MemMapError, MemoryBacking};
use super::super::switching::get_current_id;
use crate::io::async_io::AsyncOpID;
use crate::io::driver::pending::send_async_request;
use crate::io::filesystem::driver::DriverID;
use crate::io::filesystem::driver_create_mapping;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::physical::{maybe_add_frame_reference, release_tracked_frame};
use crate::memory::shared::share_buffer;
use crate::task::memory::{untrack_file_backed_page, UnmappedRegionKind};
use crate::task::paging::{
    current_pagedir_unmap, page_on_demand, ExternalPageDirectory, PermissionFlags,
};

pub fn map_memory(
    addr: Option<VirtualAddress>,
    size: u32,
    backing: MemoryBacking,
) -> Result<VirtualAddress, MemMapError> {
    map_memory_for_task(get_current_id(), addr, size, backing)
}

pub fn map_memory_for_task(
    task_id: TaskID,
    addr: Option<VirtualAddress>,
    size: u32,
    backing: MemoryBacking,
) -> Result<VirtualAddress, MemMapError> {
    let direct_mapping = match backing {
        MemoryBacking::Direct(paddr) => Some(paddr),
        _ => None,
    };
    let mapped_to = {
        let task_lock = get_task(task_id).ok_or(MemMapError::NoTask)?;
        let mut task = task_lock.write();
        task.memory_mapping.map_memory(addr, size, backing)?
    };

    if let Some(paddr) = direct_mapping {
        // Make sure the page reference is tracked. We can't rely on page
        // faults to do this.
        let pagedir = ExternalPageDirectory::for_task(task_id);
        // We haven't implemented flags for the userspace api yet, so just
        // give it all the permissions
        let flags =
            PermissionFlags::new(PermissionFlags::USER_ACCESS | PermissionFlags::WRITE_ACCESS);

        let mut offset = 0;
        while offset < size {
            let mapped_addr = mapped_to + offset;
            pagedir.map(mapped_addr, paddr + offset, flags);
            maybe_add_frame_reference(paddr + offset);
            offset += 0x1000;
        }
    }

    Ok(mapped_to)
}

pub fn remap_memory_for_task(
    task_id: TaskID,
    addr: VirtualAddress,
    backing: MemoryBacking,
) -> Result<MemoryBacking, MemMapError> {
    let task_lock = get_task(task_id).ok_or(MemMapError::NoTask)?;
    let mut task = task_lock.write();
    let mapping = task
        .memory_mapping
        .get_mut_mapping_containing_address(&addr)
        .ok_or(MemMapError::NotMapped)?;
    Ok(core::mem::replace(&mut mapping.backed_by, backing))
}

pub fn unmap_memory(addr: VirtualAddress, size: u32) -> Result<(), MemMapError> {
    unmap_memory_for_task(get_current_id(), addr, size)
}

pub fn unmap_memory_for_task(
    task_id: TaskID,
    addr: VirtualAddress,
    size: u32,
) -> Result<(), MemMapError> {
    let unmapped_regions = {
        let task_lock = get_task(task_id).ok_or(MemMapError::NoTask)?;
        let mut task = task_lock.write();
        task.memory_mapping.unmap_memory(addr, size)?
    };
    if task_id == get_current_id() {
        for region in unmapped_regions {
            let mut offset = 0;
            while offset < region.size {
                let mapping = region.address + offset;
                if let Some(frame) = current_pagedir_unmap(mapping) {
                    let released =
                        release_tracked_frame(frame).map_err(|_| MemMapError::KernelError)?;
                    if released {
                        if let UnmappedRegionKind::FileBacked {
                            driver_id,
                            mapping_token,
                            offset_in_file,
                            shared,
                        } = region.kind
                        {
                            if shared {
                                // If the frame was shared, we need to tell the driver that it's no longer mapped
                                // TODO: send request to driver

                                // If we dropped the frame used for a file-backed
                                // mapping, we also need to clear the re-use cache
                                untrack_file_backed_page(driver_id, mapping_token, offset_in_file);
                            }
                        }
                    }
                }
                offset += 0x1000;
            }
        }
    } else {
        let pagedir = ExternalPageDirectory::for_task(task_id);
        for region in unmapped_regions {
            let mut offset = 0;
            while offset < size {
                let mapping = region.address + offset;
                if let Some(frame) = pagedir.unmap(mapping) {
                    let released =
                        release_tracked_frame(frame).map_err(|_| MemMapError::KernelError)?;
                    if released {
                        if let UnmappedRegionKind::FileBacked {
                            driver_id,
                            mapping_token,
                            offset_in_file,
                            shared,
                        } = region.kind
                        {
                            if shared {
                                // If the frame was shared, we need to tell the driver that it's no longer mapped
                                // TODO: send request to driver
                                // If we dropped the frame used for a file-backed mapping, we also need to clear the re-use cache
                                untrack_file_backed_page(driver_id, mapping_token, offset_in_file);
                            }
                        }
                    }
                }
                offset += 4096;
            }
        }
    }

    Ok(())
}

/// Convenience struct for allocating a DMA range
pub struct DmaRange {
    pub vaddr_start: VirtualAddress,
    pub paddr_start: PhysicalAddress,
    pub page_count: usize,
}

impl DmaRange {
    /// Construct a DMA range containing at least this many bytes. DMA ranges
    /// are rounded up to the nearest page size
    pub fn for_byte_length(bytes: usize) -> Result<Self, MemMapError> {
        let mut length = bytes;
        if length & 0xfff != 0 {
            length &= 0xfffff000;
            length += 0x1000;
        }
        let page_count = length / 0x1000;
        Self::with_page_count(page_count)
    }

    pub fn with_page_count(page_count: usize) -> Result<Self, MemMapError> {
        let size = page_count as u32 * 0x1000;
        let vaddr_start = map_memory(None, size, MemoryBacking::IsaDma)?;
        // Paging a DMA-backed region ensures that the backing frames are
        // physically contiguous
        let paddr_start = page_on_demand(vaddr_start).ok_or(MemMapError::MappingFailed)?;

        Ok(Self {
            vaddr_start,
            paddr_start,
            page_count,
        })
    }
}

pub fn map_file(
    vaddr: Option<VirtualAddress>,
    size: u32,
    path: &str,
    offset_in_file: u32,
    shared: bool,
) -> Result<VirtualAddress, MemMapError> {
    // Mapping a file requires an async IO request to initialize the mapping
    // and get back a token. This requires the syscall to suspend the current
    // task until IO is complete. We don't want to be holding any locks when
    // we do that.
    let task_id = get_current_id();
    let task_lock = get_task(task_id).ok_or(MemMapError::NoTask)?;

    let (driver_id, relative_path) =
        crate::io::prepare_file_path(path).map_err(|_| MemMapError::FileUnavailable)?;

    let result = match driver_create_mapping(driver_id, relative_path) {
        Some(immediate) => immediate,
        None => {
            // no immediate result, need to suspend task and wait for async io
            task_lock.write().begin_file_mapping_request();
            crate::task::actions::yield_coop();
            // once awake, get token from task state and complete mapping
            let mut task = task_lock.write();
            let last_result = task.last_map_result.take();
            let Some(result) = last_result else {
                return Err(MemMapError::FileUnavailable);
            };
            result
        }
    };
    let mapping_token = match result {
        Ok(token) => DriverMappingToken::new(token),
        Err(_) => return Err(MemMapError::DriverError),
    };

    let backing = MemoryBacking::FileBacked {
        driver_id,
        mapping_token,
        offset_in_file,
        shared,
    };

    let result = task_lock
        .write()
        .memory_mapping
        .map_memory(vaddr, size, backing);

    result
}

#[cfg(test)]
mod tests {
    #[test_case]
    fn test_mmap_file_async_driver() {
        // map a frame of memory to a file on the ATEST: drive
        let vaddr = super::map_file(None, 0x1000, "ATEST:\\MYFILE", 0, true).unwrap();
        let buffer = unsafe { core::slice::from_raw_parts(vaddr.as_ptr::<u8>(), 0x1000) };
        let start_slice = &buffer[0..9];
        assert_eq!(start_slice, b"PAGE DATA");
    }

    #[test_case]
    fn test_mmap_file_invalid_path() {
        let result = super::map_file(None, 0x1000, "NONEXISTENT:\\FILE", 0, true);
        assert!(result.is_err());
    }

    #[test_case]
    fn test_mmap_file_offset_in_file() {
        // DEV:\\ASYNCDEV is a driver that, when mapped, returns the letters
        // A-Z in sequence, one letter per page

        let vaddr = super::map_file(None, 0x1000, "DEV:\\ASYNCDEV", 0xf00, true).unwrap();
        let buffer = unsafe { core::slice::from_raw_parts(vaddr.as_ptr::<u8>(), 0x1000) };
        assert_eq!(buffer[0], b'A');
        assert_eq!(buffer[0xff], b'A');
        assert_eq!(buffer[0x100], b'B');
    }

    #[test_case]
    fn test_mmap_file_same_file_shares_frame() {
        // Two mappings of the same file at the same offset should share the
        // same physical frame, rather than allocating two separate frames.
        // ATEST: returns the same mapping token for identical paths,
        // so both mappings reference the same backing file data.
        let vaddr1 = super::map_file(None, 0x1000, "ATEST:\\SHARE_TEST", 0, true).unwrap();
        let buffer1 = unsafe { core::slice::from_raw_parts(vaddr1.as_ptr::<u8>(), 0x1000) };
        assert_eq!(&buffer1[0..9], b"PAGE DATA");

        let vaddr2 = super::map_file(None, 0x1000, "ATEST:\\SHARE_TEST", 0, true).unwrap();
        let buffer2 = unsafe { core::slice::from_raw_parts(vaddr2.as_ptr::<u8>(), 0x1000) };
        assert_eq!(&buffer2[0..9], b"PAGE DATA");

        // Both virtual addresses should be backed by the same physical frame
        let paddr1 =
            crate::task::paging::maybe_get_current_physical_address(vaddr1).unwrap();
        let paddr2 =
            crate::task::paging::maybe_get_current_physical_address(vaddr2).unwrap();
        assert_eq!(paddr1, paddr2);
    }

    #[test_case]
    fn test_mmap_file_different_offsets_different_frames() {
        // Two mappings of the same file at different page-aligned offsets
        // should get different physical frames, because they contain
        // different data.
        let vaddr1 = super::map_file(None, 0x1000, "DEV:\\ASYNCDEV", 0, true).unwrap();
        let buffer1 = unsafe { core::slice::from_raw_parts(vaddr1.as_ptr::<u8>(), 0x1000) };
        assert_eq!(buffer1[0], b'A');

        let vaddr2 = super::map_file(None, 0x1000, "DEV:\\ASYNCDEV", 0x1000, true).unwrap();
        let buffer2 = unsafe { core::slice::from_raw_parts(vaddr2.as_ptr::<u8>(), 0x1000) };
        assert_eq!(buffer2[0], b'B');

        let paddr1 =
            crate::task::paging::maybe_get_current_physical_address(vaddr1).unwrap();
        let paddr2 =
            crate::task::paging::maybe_get_current_physical_address(vaddr2).unwrap();
        assert_ne!(paddr1, paddr2);
    }

    #[test_case]
    fn test_mmap_file_cross_task_shares_frame() {
        use core::sync::atomic::{AtomicU32, Ordering};
        use crate::task::actions::io::read_sync;

        static CHILD_PADDR: AtomicU32 = AtomicU32::new(0);

        // Parent maps a file and triggers paging
        let vaddr = super::map_file(None, 0x1000, "ATEST:\\CROSS_TASK", 0, true).unwrap();
        let buffer = unsafe { core::slice::from_raw_parts(vaddr.as_ptr::<u8>(), 0x1000) };
        assert_eq!(&buffer[0..9], b"PAGE DATA");
        let parent_paddr =
            crate::task::paging::maybe_get_current_physical_address(vaddr).unwrap();

        fn child_body() -> ! {
            // Child maps the same file. Because the parent already paged it,
            // the tracker should return the same physical frame.
            let vaddr = crate::task::actions::memory::map_file(
                None, 0x1000, "ATEST:\\CROSS_TASK", 0, true,
            ).unwrap();
            let buffer = unsafe { core::slice::from_raw_parts(vaddr.as_ptr::<u8>(), 0x1000) };
            assert_eq!(&buffer[0..9], b"PAGE DATA");

            let paddr =
                crate::task::paging::maybe_get_current_physical_address(vaddr).unwrap();
            CHILD_PADDR.store(paddr.as_u32(), Ordering::SeqCst);
            crate::task::actions::lifecycle::terminate(0);
        }

        let (handle, _) = crate::task::actions::handle::create_kernel_task(
            child_body,
            Some("CHILD"),
        );
        read_sync(handle, &mut [], 0).unwrap();

        let child_paddr = crate::memory::address::PhysicalAddress::new(
            CHILD_PADDR.load(Ordering::SeqCst),
        );
        assert_eq!(parent_paddr, child_paddr);
    }

    #[test_case]
    fn test_mmap_file_unmap_then_remap_gets_new_frame() {
        // After unmapping a file-backed region, the tracked frame should be
        // cleared (assuming ref count drops to zero). Re-mapping and paging
        // the same file should allocate a fresh frame.
        // Uses a unique path so this test gets its own mapping token,
        // avoiding interference from other tests that map "MYFILE".
        let vaddr1 = super::map_file(None, 0x1000, "ATEST:\\UNMAP_TEST", 0, true).unwrap();
        let buffer1 = unsafe { core::slice::from_raw_parts(vaddr1.as_ptr::<u8>(), 0x1000) };
        assert_eq!(&buffer1[0..9], b"PAGE DATA");
        let paddr1 =
            crate::task::paging::maybe_get_current_physical_address(vaddr1).unwrap();

        // Unmap the region — this should release the frame and untrack it
        super::unmap_memory(vaddr1, 0x1000).unwrap();

        // Re-map the same file
        let vaddr2 = super::map_file(None, 0x1000, "ATEST:\\UNMAP_TEST", 0, true).unwrap();
        let buffer2 = unsafe { core::slice::from_raw_parts(vaddr2.as_ptr::<u8>(), 0x1000) };
        assert_eq!(&buffer2[0..9], b"PAGE DATA");
        let paddr2 =
            crate::task::paging::maybe_get_current_physical_address(vaddr2).unwrap();

        // The frame should be different because the old one was untracked
        assert_ne!(paddr1, paddr2);
    }

    #[test_case]
    fn test_mmap_file_private_gets_separate_frames() {
        // Two private mappings of the same file at the same offset should
        // get different physical frames, since private mappings skip the
        // shared page tracker entirely.
        let vaddr1 = super::map_file(None, 0x1000, "ATEST:\\PRIV_TEST", 0, false).unwrap();
        let buffer1 = unsafe { core::slice::from_raw_parts(vaddr1.as_ptr::<u8>(), 0x1000) };
        assert_eq!(&buffer1[0..9], b"PAGE DATA");

        let vaddr2 = super::map_file(None, 0x1000, "ATEST:\\PRIV_TEST", 0, false).unwrap();
        let buffer2 = unsafe { core::slice::from_raw_parts(vaddr2.as_ptr::<u8>(), 0x1000) };
        assert_eq!(&buffer2[0..9], b"PAGE DATA");

        let paddr1 =
            crate::task::paging::maybe_get_current_physical_address(vaddr1).unwrap();
        let paddr2 =
            crate::task::paging::maybe_get_current_physical_address(vaddr2).unwrap();
        assert_ne!(paddr1, paddr2);
    }

    #[test_case]
    fn test_mmap_file_private_is_writable() {
        // Private file-backed mappings should be writable. Writing to the
        // mapping should not panic or fault.
        let vaddr = super::map_file(None, 0x1000, "ATEST:\\PRIV_WRITE", 0, false).unwrap();
        let buffer = unsafe { core::slice::from_raw_parts_mut(vaddr.as_ptr_mut::<u8>(), 0x1000) };
        assert_eq!(&buffer[0..9], b"PAGE DATA");

        // Overwrite the start of the private mapping
        buffer[0] = b'X';
        buffer[1] = b'Y';
        assert_eq!(buffer[0], b'X');
        assert_eq!(buffer[1], b'Y');
        // The rest of the original data is still intact
        assert_eq!(&buffer[2..9], b"GE DATA");
    }
}
