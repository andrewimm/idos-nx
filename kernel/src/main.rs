#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

pub mod arch;
pub mod init;
pub mod interrupts;
pub mod io;
pub mod log;
pub mod panic;

#[no_mangle]
pub extern "C" fn _start() -> ! {

    unsafe {
        init::zero_bss();
        init::init_cpu_tables();
        init::init_memory();
    }

    kprint!("\nKernel Memory initialized.\n");

    loop {}
}
