#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(custom_test_frameworks)]

#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::arch::asm;

extern crate alloc;

pub mod arch;
pub mod hardware;
pub mod init;
pub mod interrupts;
pub mod io;
pub mod log;
pub mod memory;
pub mod panic;
pub mod task;
pub mod time;

#[no_mangle]
pub extern "C" fn _start() -> ! {

    unsafe {
        init::zero_bss();
        init::init_cpu_tables();
        init::init_memory();
    }

    kprint!("\nKernel Memory initialized.\n");

    init::init_hardware();

    #[cfg(test)]
    test_main();

    {
        let mut b = alloc::vec::Vec::new();
        for i in 0..5 {
            b.push(i);
        }
        kprint!("Allocated: {}\n", b.len());
    }

    loop {
        unsafe {
            asm!(
                "sti",
                "hlt",
            );
        }
    }
}


#[cfg(test)]
fn test_runner(tests: &[&dyn Fn()]) {
    kprint!("Running {} tests\n", tests.len());
    for test in tests {
        kprint!("... ");
        test();
        kprint!("[ok]\n");
    }
    loop {}
}

