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
pub mod collections;
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
        let cur_id = task::switching::get_current_id();
        let task_id = task::switching::get_next_id();
        let task_stack = task::stack::allocate_stack();
        let mut task_a = task::state::Task::new(task_id, cur_id, task_stack);
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

fn wait_task_body() -> ! {
    kprint!("Child Task\n");
    task::sleep(2000);
    //task::lifecycle::terminate(16);
    unsafe {
        asm!("xor dx, dx; div dx");
    }
    loop {}
}

fn task_a_body() -> ! {
    let cur_id = task::switching::get_current_id();
    let wait_id = task::switching::get_next_id();

    {
        let task_stack = task::stack::allocate_stack();
        let mut wait_task = task::state::Task::new(wait_id, cur_id, task_stack);
        wait_task.set_entry_point(wait_task_body);
        wait_task.make_runnable();
        task::switching::insert_task(wait_task);

        let return_code = task::lifecycle::wait_for_child(wait_id, None);
        kprint!("Child Task returned: {}\n", return_code);
    }

    let b_id = task::switching::get_next_id();
    {
        let stack = task::stack::allocate_stack();
        let mut task_b = task::state::Task::new(b_id, cur_id, stack);
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

