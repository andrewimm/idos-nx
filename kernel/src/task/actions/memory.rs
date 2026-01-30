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
    {
        let task_lock = get_task(task_id).ok_or(MemMapError::NoTask)?;
        let mut task = task_lock.write();
        task.memory_mapping.unmap_memory(addr, size)?;
    }
    if task_id == get_current_id() {
        let mut offset = 0;
        while offset < size {
            let mapping = addr + offset;
            if let Some(frame) = current_pagedir_unmap(mapping) {
                release_tracked_frame(frame);
            }
            offset += 4096;
        }
    } else {
        let pagedir = ExternalPageDirectory::for_task(task_id);
        let mut offset = 0;
        while offset < size {
            let mapping = addr + offset;
            if let Some(frame) = pagedir.unmap(mapping) {
                release_tracked_frame(frame);
            }
            offset += 4096;
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
        let vaddr = super::map_file(None, 0x1000, "ATEST:\\MYFILE", 0).unwrap();
        let buffer = unsafe { core::slice::from_raw_parts(vaddr.as_ptr::<u8>(), 0x1000) };
        let start_slice = &buffer[0..9];
        assert_eq!(start_slice, b"PAGE DATA");
    }

    #[test_case]
    fn test_mmap_file_invalid_path() {
        let result = super::map_file(None, 0x1000, "NONEXISTENT:\\FILE", 0);
        assert!(result.is_err());
    }

    #[test_case]
    fn test_mmap_file_offset_in_file() {
        // DEV:\\ASYNCDEV is a driver that, when mapped, returns the letters
        // A-Z in sequence, one letter per page

        let vaddr = super::map_file(None, 0x1000, "DEV:\\ASYNCDEV", 0xf00).unwrap();
        let buffer = unsafe { core::slice::from_raw_parts(vaddr.as_ptr::<u8>(), 0x1000) };
        assert_eq!(buffer[0], b'A');
        assert_eq!(buffer[0xff], b'A');
        assert_eq!(buffer[0x100], b'B');
    }
}
