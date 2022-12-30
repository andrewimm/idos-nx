#![no_std]
#![no_main]

mod video;

#[no_mangle]
#[link_section = ".entry"]
pub extern "C" fn _start() -> ! {
    video::print_string("... IDOS BOOTBIN");

    loop {}
}

#[panic_handler]
pub fn panic(info: &core::panic::PanicInfo) -> ! {
    video::print_string("PANIC");

    loop {}
}
