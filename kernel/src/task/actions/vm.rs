use idos_api::compat::VMRegisters;

use crate::{interrupts::syscall::SavedRegisters, task::switching::get_current_task};

pub fn enter_vm86_mode(registers: &SavedRegisters, vm_regs_ptr: *mut VMRegisters) {
    let task_lock = get_current_task();

    task_lock.write().vm86_registers = Some(registers.clone());

    let vm_regs = unsafe { &mut *vm_regs_ptr };

    crate::kprintln!("Enter 8086 Mode @ {:X}:{:X}", vm_regs.cs, vm_regs.eip);
    loop {}
}
