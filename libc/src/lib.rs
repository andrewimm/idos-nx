#![no_std]
#![feature(c_variadic)]
#![feature(lang_items)]

mod allocator;
mod ctype;
mod dirent;
mod errno;
mod locale;
mod math;
mod mman;
mod setjmp;
mod signal;
mod stat;
mod stdio;
mod stdlib;
mod string;
mod termios;
mod time;
mod unistd;

use core::ffi::c_int;

/// Called by crt0.s before main() to initialize libc state.
#[no_mangle]
pub extern "C" fn __libc_init() {
    allocator::init();
    stdio::init();
    unsafe { stdio::init_std_pointers(); }
}

#[lang = "eh_personality"]
pub extern "C" fn eh_personality() {}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    idos_api::syscall::exec::terminate(0xff)
}
