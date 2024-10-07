use crate::memory::address::{VirtualAddress, PhysicalAddress};
use crate::task::paging::{ExternalPageDirectory, current_pagedir_unmap, page_on_demand};
use super::super::id::TaskID;
use super::super::memory::{MemoryBacking, TaskMemoryError};
use super::super::switching::{get_current_id, get_task};

pub fn map_memory(addr: Option<VirtualAddress>, size: u32, backing: MemoryBacking) -> Result<VirtualAddress, TaskMemoryError> {
    map_memory_for_task(get_current_id(), addr, size, backing)
}

pub fn map_memory_for_task(task_id: TaskID, addr: Option<VirtualAddress>, size: u32, backing: MemoryBacking) -> Result<VirtualAddress, TaskMemoryError> {
    let task_lock = get_task(task_id).ok_or(TaskMemoryError::NoTask)?;
    let mut task = task_lock.write();
    task.memory_mapping.map_memory(addr, size, backing)
}

pub fn remap_memory_for_task(task_id: TaskID, addr: VirtualAddress, backing: MemoryBacking) -> Result<MemoryBacking, TaskMemoryError> {
    let task_lock = get_task(task_id).ok_or(TaskMemoryError::NoTask)?;
    let mut task = task_lock.write();
    let mapping = task.memory_mapping.get_mut_mapping_containing_address(&addr).ok_or(TaskMemoryError::NotMapped)?;
    Ok(core::mem::replace(&mut mapping.backed_by, backing))
}

pub fn unmap_memory_for_task(task_id: TaskID, addr: VirtualAddress, size: u32) -> Result<(), TaskMemoryError> {
    {
        let task_lock = get_task(task_id).ok_or(TaskMemoryError::NoTask)?;
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
    pub fn for_byte_length(bytes: usize) -> Result<Self, TaskMemoryError> {
        let mut length = bytes;
        if length & 0xfff != 0 {
            length &= 0xfffff000;
            length += 0x1000;
        }
        let page_count = length / 0x1000;
        Self::with_page_count(page_count)
    }

    pub fn with_page_count(page_count: usize) -> Result<Self, TaskMemoryError> {
        let size = page_count as u32 * 0x1000;
        let vaddr_start = map_memory(None, size, MemoryBacking::DMA)?;
        let paddr_start = page_on_demand(vaddr_start).ok_or(TaskMemoryError::MappingFailed)?;

        Ok(
            Self {
                vaddr_start,
                paddr_start,
                page_count,
            }
        )
    }
}

