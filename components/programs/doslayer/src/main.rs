//! In order for DOS programs to run, IDOS needs to put a DOS compatibility
//! layer in higher userspace memory. This 32-bit code runs in a loop, entering
//! a 8086 VM before returning on an interrupt or GPF fault.

#![no_std]
#![no_main]
#![feature(lang_items)]

extern crate idos_api;

pub mod panic;
use core::arch::{asm, global_asm};

global_asm!(
    r#"
.global _start

_start:
    mov edi, [esp]
    lea esi, [esp + 4]
    push esi
    push edi
    call compat_start
"#
);

#[no_mangle]
pub extern "C" fn compat_start(_argc: u32, _argv: *const u32) {
    //env::init_args(argc, argv);

    let psp_segment: u16;
    unsafe {
        asm!(
            "mov {psp}, fs",
            psp = out(reg) psp_segment,
        );
    }

    loop {
        idos_api::syscall::exec::enter_8086();
    }

    idos_api::syscall::exec::terminate(0)
}
