#![no_std]
#![feature(alloc_error_handler)]
#![feature(lang_items)]

pub mod allocator;
pub mod env;
pub mod log;
pub mod panic;

extern crate alloc;
extern crate idos_api;

use core::arch::global_asm;

extern "C" {
    fn main();
}

global_asm!(
    r#"
.global _start

_start:
    mov edi, [esp]
    lea esi, [esp + 4]
    push esi
    push edi
    call sdk_start
"#
);

#[no_mangle]
pub extern "C" fn sdk_start(argc: u32, argv: *const u32) {
    env::init_args(argc, argv);

    unsafe { main() };

    idos_api::syscall::exec::terminate(0)
}
