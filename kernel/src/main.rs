#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(const_btree_new)]
#![feature(const_mut_refs)]
#![feature(custom_test_frameworks)]
#![feature(naked_functions)]
#![feature(vec_into_raw_parts)]

#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::arch::asm;

use crate::memory::virt::page_table::get_current_pagedir;

extern crate alloc;

pub mod arch;
pub mod cleanup;
pub mod collections;
pub mod command;
pub mod console;
pub mod devices;
pub mod files;
pub mod filesystem;
pub mod hardware;
pub mod init;
pub mod interrupts;
pub mod io;
pub mod loader;
pub mod log;
pub mod memory;
pub mod panic;
pub mod pipes;
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

    let initial_pagedir = get_current_pagedir();
    task::switching::init(initial_pagedir);

    task::actions::lifecycle::create_kernel_task(cleanup::cleanup_task, Some("CLEANUP"));

    filesystem::init_fs();

    init::init_device_drivers();

    #[cfg(test)]
    {
        task::actions::lifecycle::create_kernel_task(run_tests, Some("TESTS"));
    }
    
    #[cfg(not(test))]
    {
        task::actions::lifecycle::create_kernel_task(init_system, Some("INIT"));
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

fn init_system() -> ! {
    let id = task::switching::get_current_id();
    crate::kprintln!("INIT task: {:?}", id);
    // initialize drivers that rely on multitasking
    {
        hardware::ps2::install_drivers();

        crate::kprintln!("Query ATA bus...");
        hardware::ata::dev::install_drivers();

        hardware::floppy::dev::install_drivers();

        hardware::ethernet::dev::install_driver();

        filesystem::drivers::fatfs::mount_fat_fs();

        console::init_console();
    }
    // do other boot stuff
    // right now this just runs demos / tests
    task_a_body();
}

fn task_a_body() -> ! {
    let mut buf: [u8; 5] = [b'A'; 5];
    let exec_child = task::actions::lifecycle::create_task();
    task::actions::lifecycle::attach_executable_to_task(exec_child, "A:\\TEST.BIN");
    task::actions::lifecycle::wait_for_child(exec_child, None);


    crate::kprint!("\n\nReading from COM1:\n");
    let com1 = task::actions::io::open_path("DEV:\\COM1").unwrap();
    let read_len = task::actions::io::read_file(com1, &mut buf).unwrap() as usize;
    let res = core::str::from_utf8(&buf[..read_len]).unwrap();
    kprint!("Read {} bytes from COM1. Value was \"{}\"\n", read_len, res);
    crate::kprint!("Write to COM...\n");
    task::actions::io::write_file(com1, "HELLO COM\n".as_bytes()).unwrap();
    crate::kprint!("\n");
    task::actions::io::close_file(com1).unwrap();

    let b_id = task::actions::lifecycle::create_kernel_task(task_b_body, Some("TASK B"));

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
fn run_tests() -> ! {
    test_main();
    loop {}
}

#[cfg(test)]
fn test_runner(tests: &[&dyn Fn()]) -> ! {
    kprint!("Running {} tests\n", tests.len());
    for test in tests {
        kprint!("... ");
        test();
        kprint!("[ok]\n");
    }
    kprint!("All tests passed!\n");
    kprint!("Exiting in 5 seconds\n");
    task::actions::sleep(5000);
    hardware::qemu::debug_exit(0);
}

