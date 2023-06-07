use crate::memory::address::VirtualAddress;
use super::super::id::TaskID;
use super::super::memory::{MemoryBacking, TaskMemoryError};
use super::super::switching::{get_current_id, get_task};

pub fn map_memory(addr: Option<VirtualAddress>, size: u32, backing: MemoryBacking) -> Result<VirtualAddress, TaskMemoryError> {
    map_memory_for_task(get_current_id(), addr, size, backing)
}

pub fn map_memory_for_task(task_id: TaskID, addr: Option<VirtualAddress>, size: u32, backing: MemoryBacking) -> Result<VirtualAddress, TaskMemoryError> {
    let task_lock = get_task(task_id).ok_or(TaskMemoryError::NoTask)?;
    let mut task = task_lock.write();
    task.memory_mapping.map_memory(None, size, backing)
}

