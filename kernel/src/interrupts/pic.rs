use core::arch::global_asm;
use crate::hardware::pic::PIC;
use super::stack::{SavedState, StackFrame};

global_asm!(r#"
.global pic_irq_0, pic_irq_1, pic_irq_3

pic_irq_0:
    push 0x0
    jmp pic_irq_core

pic_irq_1:
    push 0x1
    jmp pic_irq_core

pic_irq_3:
    push 0x3
    jmp pic_irq_core

pic_irq_4:
    push 0x4
    jmp pic_irq_core

pic_irq_5:
    push 0x5
    jmp pic_irq_core

pic_irq_6:
    push 0x6
    jmp pic_irq_core

pic_irq_7:
    push 0x7
    jmp pic_irq_core

pic_irq_8:
    push 0x8
    jmp pic_irq_core

pic_irq_9:
    push 0x9
    jmp pic_irq_core

pic_irq_a:
    push 0xa
    jmp pic_irq_core

pic_irq_b:
    push 0xb
    jmp pic_irq_core

pic_irq_c:
    push 0xc
    jmp pic_irq_core

pic_irq_d:
    push 0xd
    jmp pic_irq_core

pic_irq_e:
    push 0xe
    jmp pic_irq_core

pic_irq_f:
    push 0xf
    jmp pic_irq_core

# called once the serviced IRQ number has been pushed onto the stack,
pic_irq_core:
    push eax
    push ecx
    push edx
    push ebx
    push ebp
    push esi
    push edi

    call _handle_pic_interrupt

    pop edi
    pop esi
    pop ebp
    pop ebx
    pop edx
    pop ecx
    pop eax
    add esp, 4 # clear the irq number

    iretd
"#);

/// Handle interrupts that come from the PIC
#[no_mangle]
pub extern "C" fn _handle_pic_interrupt(registers: SavedState, irq: u32, frame: StackFrame) {
    let pic = PIC::new();

    if irq == 0 {
        // IRQ 0 is not installable, and is hard-coded to the kernel's PIT
        // interrupt handler
        handle_pit_interrupt();
    }

    // need to check 7 and 15 for spurious interrupts
    if irq == 7 {
        let serviced = pic.get_interrupts_in_service();
        if serviced & 0x80 == 0 {
            return;
        }
    }
    if irq == 15 {
        let serviced = pic.get_interrupts_in_service();
        if serviced & 0x8000 == 0 {
            pic.acknowledge_interrupt(2);
            return;
        }
    }

    pic.acknowledge_interrupt(irq as u8);
}

/// The PIT triggers at 100Hz, and is used to update the internal clock and the
/// task scheduler.
pub fn handle_pit_interrupt() {
    crate::time::system::tick();
}
