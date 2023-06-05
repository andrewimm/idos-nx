use crate::memory::address::VirtualAddress;
use super::super::memory::{MemoryBacking, TaskMemoryError};
use super::super::switching::get_current_task;

pub fn map_memory(addr: Option<VirtualAddress>, size: u32, backing: MemoryBacking) -> Result<VirtualAddress, TaskMemoryError> {
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    task.memory_mapping.map_memory(None, size, backing)
}
