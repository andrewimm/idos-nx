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

    let mut vm_regs = VMRegisters {
        eax: 0,
        ebx: 0,
        ecx: 0,
        edx: 0,
        esi: 0,
        edi: 0,
        ebp: 0,
        eip: 0x4089,
        cs: psp_segment as u32,
        ds: psp_segment as u32,
        es: psp_segment as u32,
        ss: psp_segment as u32,
    };

    loop {
        idos_api::syscall::exec::enter_8086(&mut vm_regs);
    }

    idos_api::syscall::exec::terminate(0)
}
