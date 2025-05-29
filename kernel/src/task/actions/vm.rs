use idos_api::compat::VMRegisters;

use crate::{interrupts::syscall::SavedRegisters, task::switching::get_current_task};

use core::arch::asm;

pub fn enter_vm86_mode(registers: &SavedRegisters, vm_regs_ptr: *mut VMRegisters) {
    let task_lock = get_current_task();

    task_lock.write().vm86_registers = Some(registers.clone());

    let vm_regs = unsafe { &mut *vm_regs_ptr };
    vm_regs.eflags |= 0x20000;

    let vm_regs_copy = vm_regs.clone();

    crate::kprintln!("Enter 8086 Mode @ {:X}:{:X}", vm_regs.cs, vm_regs.eip);

    unsafe {
        asm!(
            "mov esp, eax",
            "pop eax",
            "pop ebx",
            "pop ecx",
            "pop edx",
            "pop esi",
            "pop edi",
            "pop ebp",
            "iretd",
            in("eax") &vm_regs_copy as *const VMRegisters as u32
        )
    }
}
