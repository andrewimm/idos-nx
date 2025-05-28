use crate::{interrupts::syscall::SavedRegisters, task::switching::get_current_task};

pub fn enter_vm86_mode(registers: &SavedRegisters) {
    let task_lock = get_current_task();

    task_lock.write().vm86_registers = Some(registers.clone());
}
