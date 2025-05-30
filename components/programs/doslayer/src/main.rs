//! In order for DOS programs to run, IDOS needs to put a DOS compatibility
//! layer in higher userspace memory. This 32-bit code runs in a loop, entering
//! a 8086 VM before returning on an interrupt or GPF fault.

#![no_std]
#![no_main]
#![feature(lang_items)]

extern crate idos_api;

pub mod panic;
use core::arch::{asm, global_asm};

use idos_api::compat::VMRegisters;

global_asm!(
    r#"
.global _start

_start:
    mov edi, [esp]
    lea esi, [esp + 4]
    push esi
    push edi
    push eax
    call compat_start
"#
);

#[no_mangle]
pub extern "C" fn compat_start(psp_segment: u32, _argc: u32, _argv: *const u32) {
    //env::init_args(argc, argv);

    let stdout = idos_api::io::handle::Handle::new(1);

    let mut vm_regs = VMRegisters {
        eax: 0xaa,
        ebx: 0xbb,
        ecx: 0xcc,
        edx: 0xdd,
        esi: 0xee,
        edi: 0xff,
        ebp: 0xab,
        eip: 0,
        esp: 0xfae,
        eflags: 0x2,
        cs: psp_segment as u32,
        ss: psp_segment as u32,
    };

    loop {
        idos_api::syscall::exec::enter_8086(&mut vm_regs);

        unsafe {
            if !handle_fault(&mut vm_regs) {
                break;
            }
        }
    }

    idos_api::syscall::exec::terminate(0)
}

unsafe fn handle_fault(vm_regs: &mut VMRegisters) -> bool {
    let mut op_ptr = ((vm_regs.cs << 4) + vm_regs.eip) as *const u8;
    // TODO: check prefix
    match *op_ptr {
        0x9c => { // PUSHF
        }
        0x9d => { // POPF
        }
        0xcd => {
            // INT
            let irq = *op_ptr.add(1);
            handle_interrupt(irq, vm_regs);
            vm_regs.eip += 2;
            return true;
        }
        0xcf => { // IRET
        }
        0xf4 => { // HLT
        }
        0xfa => { // CLI
        }
        0xfb => { // STI
        }
        _ => (),
    }

    false
}

fn handle_interrupt(irq: u8, vm_regs: &mut VMRegisters) {
    match irq {
        // So many interrupts to implement here...
        0x21 => {
            // DOS API
            dos_api(vm_regs);
        }

        // TODO: jump to the value in the IVT, or fail if there is no irq
        _ => (),
    }
}

fn dos_api(vm_regs: &mut VMRegisters) {
    match vm_regs.ah() {
        0x09 => {
            print_string(vm_regs);
        }
        _ => {
            panic!("Unsupported API")
        }
    }
}

/// AH=0x09 - Print a dollar-terminated string to STDOUT
/// Input:
///     DS:DX points to the string
/// Output:
///     None
pub fn print_string(regs: &mut VMRegisters) {}
