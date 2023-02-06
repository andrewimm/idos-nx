#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(const_btree_new)]
#![feature(const_mut_refs)]
#![feature(custom_test_frameworks)]
#![feature(naked_functions)]

#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::arch::asm;

extern crate alloc;

pub mod arch;
pub mod cleanup;
pub mod collections;
pub mod files;
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

    task::actions::lifecycle::create_kernel_task(cleanup::cleanup_task);

    #[cfg(test)]
    test_main();

    {
        task::actions::lifecycle::create_kernel_task(task_a_body);
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
    task::actions::sleep(2000);
    //task::actions::lifecycle::terminate(16);
    unsafe {
        asm!("xor dx, dx; div dx");
    }
    loop {}
}

fn task_a_body() -> ! {
    {
        let wait_id = task::actions::lifecycle::create_kernel_task(wait_task_body);
        let return_code = task::actions::lifecycle::wait_for_child(wait_id, None);
        kprint!("Child task returned: {}\n", return_code);
    }

    let b_id = task::actions::lifecycle::create_kernel_task(task_b_body);

    use task::messaging::Message;

    loop {
        kprint!("TICK\n");
        task::actions::sleep(1000);
        task::actions::send_message(b_id, Message(0, 0, 0, 0), 0xffffffff);
        task::actions::sleep(1000);
    }
}

fn task_b_body() -> ! {
    loop {
        let _ = task::actions::read_message_blocking(None);
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

