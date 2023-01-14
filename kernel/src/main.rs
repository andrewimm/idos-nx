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
        let mut task_a = task::state::Task::new(task_id, task_stack);
        task_a.set_entry_point(task_a_body);
        task_a.make_runnable();
        task::switching::insert_task(task_a);
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

fn task_a_body() -> ! {
    let b_id = task::switching::get_next_id();
    {
        let stack = task::stack::allocate_stack();
        let mut task_b = task::state::Task::new(b_id, stack);
        task_b.set_entry_point(task_b_body);
        task_b.make_runnable();
        task::switching::insert_task(task_b);
    }

    use task::messaging::Message;

    loop {
        kprint!("TICK\n");
        task::sleep(1000);
        task::send_message(b_id, Message(0, 0, 0, 0), 0xffffffff);
        task::sleep(1000);
    }
}

fn task_b_body() -> ! {
    loop {
        let _ = task::read_message_blocking(None);
        kprint!("TOCK\n");
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

