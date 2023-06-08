use crate::memory::address::VirtualAddress;
use crate::task::paging::ExternalPageDirectory;
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

pub fn unmap_memory_for_task(task_id: TaskID, addr: VirtualAddress, size: u32) -> Result<(), TaskMemoryError> {
    {
        let task_lock = get_task(task_id).ok_or(TaskMemoryError::NoTask)?;
        let mut task = task_lock.write();
        task.memory_mapping.unmap_memory(addr, size);
    }
    if task_id == get_current_id() {
        // TODO: explicitly unmap from page table
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

