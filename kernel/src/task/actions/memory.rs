use super::super::id::TaskID;
use super::super::map::get_task;
use super::super::memory::{MemMapError, MemoryBacking};
use super::super::switching::get_current_id;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::task::paging::{current_pagedir_unmap, page_on_demand, ExternalPageDirectory};

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
    let task_lock = get_task(task_id).ok_or(MemMapError::NoTask)?;
    let mut task = task_lock.write();
    task.memory_mapping.map_memory(addr, size, backing)
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
            current_pagedir_unmap(mapping);
            offset += 4096;
        }
    } else {
        let pagedir = ExternalPageDirectory::for_task(task_id);
        let mut offset = 0;
        while offset < size {
            let mapping = addr + offset;
            pagedir.unmap(mapping);
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
