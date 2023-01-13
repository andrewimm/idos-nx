#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(const_btree_new)]
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

    task::switching::init();

    #[cfg(test)]
    test_main();

    {
        let task_id = task::switching::get_next_id();
        let task_stack = task::stack::allocate_stack();
        let mut other_task = task::state::Task::new(task_id, task_stack);
        other_task.set_entry_point(other_task_body);
        other_task.make_runnable();
        task::switching::insert_task(other_task);
    }

    loop {
        unsafe {
            asm!("cli");
            task::switching::yield_coop();
            asm!(
                "sti",
                "hlt",
            );
        }
    }
}

fn other_task_body() -> ! {
    loop {
        task::switching::yield_coop();
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

