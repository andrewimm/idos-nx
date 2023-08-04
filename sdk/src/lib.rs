#![no_std]
#![feature(alloc_error_handler)]
#![feature(lang_items)]

pub mod allocator;
pub mod driver;
pub mod panic;

extern crate alloc;
extern crate idos_api;

extern {
    fn main();
}

#[no_mangle]
pub extern "C" fn _start(argc: u32, argv0: *const u32) {
    allocator::init_allocator();

    unsafe { main() };

    idos_api::syscall::exec::terminate(0)
}

