use alloc::sync::Arc;
use spin::RwLock;

use crate::{
    io::handle::Handle,
    memory::address::VirtualAddress,
    sync::wake_set::WakeSet,
    task::{
        actions::memory::map_memory, id::TaskID, memory::MemoryBacking,
        paging::get_current_physical_address, switching::get_current_task,
    },
};

static CONSOLE_MANAGER: RwLock<Option<(TaskID, Arc<WakeSet>)>> = RwLock::new(None);

pub fn register_console_manager(wake_set: Handle) -> Result<VirtualAddress, ()> {
    if CONSOLE_MANAGER.read().is_none() {
        return Err(());
    }
    {
        let current_task_lock = get_current_task();
        let current_task = current_task_lock.write();
        let task_id = current_task.id;
        let wake_set = current_task.wake_sets.get(wake_set).cloned().ok_or(())?;
        CONSOLE_MANAGER.write().replace((task_id, wake_set));
    }

    let buffer_page = map_memory(None, 0x1000, MemoryBacking::Anonymous).map_err(|_| ())?;
    let buffer_phys =
        get_current_physical_address(buffer_page).expect("Failed to allocate CONMAN input buffer");

    Ok(buffer_page)
}

pub fn wake_console_manager() {
    if let Some((_, wake_set)) = CONSOLE_MANAGER.read().as_ref() {
        wake_set.wake();
    }
}
