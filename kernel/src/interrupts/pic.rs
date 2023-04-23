use core::arch::global_asm;
use spin::RwLock;

use crate::hardware::pic::PIC;
use super::stack::{SavedState, StackFrame};

global_asm!(r#"
.global pic_irq_0, pic_irq_1, pic_irq_3, pic_irq_4, pic_irq_5, pic_irq_6, pic_irq_7, pic_irq_8, pic_irq_9, pic_irq_a, pic_irq_b, pic_irq_c, pic_irq_d, pic_irq_e, pic_irq_f

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
pub extern "C" fn _handle_pic_interrupt(_registers: SavedState, irq: u32, _frame: StackFrame) {
    let pic = PIC::new();

    if irq == 0 {
        // IRQ 0 is not installable, and is hard-coded to the kernel's PIT
        // interrupt handler
        handle_pit_interrupt();
        pic.acknowledge_interrupt(0);
        return;
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

    let handler = try_get_installed_handler(irq);
    if let Some(f) = handler {
        f(irq);
    }

    pic.acknowledge_interrupt(irq as u8);
}

/// The PIT triggers at 100Hz, and is used to update the internal clock and the
/// task scheduler.
pub fn handle_pit_interrupt() {
    crate::time::system::tick();
    crate::task::switching::update_timeouts(crate::time::system::MS_PER_TICK);
}

pub type InstallableHandler = RwLock<Option<fn(u32) -> ()>>;

const UNINSTALLED_HANDLER: InstallableHandler = RwLock::new(None);

static INSTALLED_HANDLERS: [InstallableHandler; 16] = [UNINSTALLED_HANDLER; 16];

pub fn install_interrupt_handler(irq: u32, f: fn(u32) -> ()) {
    match INSTALLED_HANDLERS[irq as usize].try_write() {
        Some(mut inner) => {
            inner.replace(f);
        },
        None => (),
    }
}

pub fn try_get_installed_handler(irq: u32) -> Option<fn(u32) -> ()> {
    match INSTALLED_HANDLERS[irq as usize].try_read() {
        Some(inner) => *inner,
        None => None,
    }
}

