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

/// Temporary debug: called from crt0 right before main() to verify argc/argv
#[no_mangle]
pub extern "C" fn __debug_args(argc: c_int, argv: *const *const u8) {
    unsafe {
        stdio::debug_write(b"[DBG] argc=");
        stdio::debug_write_hex(argc as u32);
        stdio::debug_write(b" argv=");
        stdio::debug_write_hex(argv as u32);
        if argc > 0 && !argv.is_null() {
            let arg0 = *argv;
            stdio::debug_write(b" argv[0]=");
            stdio::debug_write_hex(arg0 as u32);
        }
        stdio::debug_write(b"\n");
    }
}

#[lang = "eh_personality"]
pub extern "C" fn eh_personality() {}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    idos_api::syscall::exec::terminate(0xff)
}
