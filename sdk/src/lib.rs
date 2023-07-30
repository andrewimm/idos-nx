#![no_std]
#![feature(lang_items)]

pub mod driver;
pub mod panic;

extern crate idos_api;

extern {
    fn main();
}

#[no_mangle]
pub extern "C" fn _start() {
    unsafe { main() };
    idos_api::syscall::exec::terminate(0)
}

