use core::arch::global_asm;

use crate::task::actions;

use super::stack::StackFrame;

global_asm!(r#"
.global syscall_handler

syscall_handler:
    push eax
    push ecx
    push edx
    push ebx
    push ebp
    push esi
    push edi
    mov ebx, esp
    push ebx
    add ebx, 7 * 4
    push ebx

    call _syscall_inner

    add esp, 8
    pop edi
    pop esi
    pop ebp
    pop ebx
    pop edx
    pop ecx
    pop eax

    iretd
"#);

#[repr(C, packed)]
pub struct SavedRegisters {
    edi: u32,
    esi: u32,
    ebp: u32,
    ebx: u32,
    edx: u32,
    ecx: u32,
    eax: u32,
}

impl core::fmt::Debug for SavedRegisters {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let eax = self.eax;
        let ebx = self.ebx;
        let ecx = self.ecx;
        let edx = self.edx;
        let ebp = self.ebp;
        let esi = self.esi;
        let edi = self.edi;
        f.debug_struct("Saved Registers")
            .field("eax", &format_args!("{:#010X}", eax))
            .field("ebx", &format_args!("{:#010X}", ebx))
            .field("ecx", &format_args!("{:#010X}", ecx))
            .field("edx", &format_args!("{:#010X}", edx))
            .field("ebp", &format_args!("{:#010X}", ebp))
            .field("esi", &format_args!("{:#010X}", esi))
            .field("edi", &format_args!("{:#010X}", edi))
            .finish()
    }
}

#[no_mangle]
pub extern "C" fn _syscall_inner(_frame: &StackFrame, registers: &mut SavedRegisters) {
    crate::kprint!("REG: {:?}\n", registers);
    let eax = registers.eax;
    match eax {
        0x00 => {
            let code = registers.ebx;
            actions::lifecycle::terminate(code);
        },
        0x05 => {
            let duration = registers.ebx;
            actions::sleep(duration);
        },
        0x06 => {
            actions::yield_coop();
        },
        0xffff => {
            crate::kprint!("\n\nSyscall: DEBUG\n");
            registers.eax = 0;
        },
        _ => panic!("Unknown Syscall!"),
    }
}
