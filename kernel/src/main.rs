#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(custom_test_frameworks)]
#![feature(map_try_insert)]
#![feature(naked_functions)]
#![feature(new_range_api)]
#![feature(vec_into_raw_parts)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::arch::asm;

extern crate alloc;

pub mod acpi;
pub mod arch;
pub mod cleanup;
pub mod collections;
pub mod command;
pub mod console;
pub mod dos;
pub mod files;
pub mod hardware;
pub mod init;
pub mod interrupts;
pub mod io;
pub mod loader;
pub mod log;
pub mod memory;
pub mod net;
pub mod panic;
pub mod pipes;
pub mod sync;
pub mod task;
pub mod time;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        init::init_cpu_tables();
        init::init_memory();
    }

    kprint!("\nKernel Memory initialized.\n");

    acpi::init();

    init::init_hardware();

    let initial_pagedir = memory::virt::page_table::get_current_pagedir();
    task::switching::init(initial_pagedir);

    task::actions::lifecycle::create_kernel_task(cleanup::cleanup_resident, Some("CLEANUPR"));

    init::init_device_drivers();

    io::init_async_io_system();

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
            asm!("sti", "hlt",);
        }
    }
}

fn system_log(console_handle: crate::io::handle::Handle, message: &str) {
    let _ = task::actions::io::write_sync(console_handle, message.as_bytes(), 0);
}

fn init_system() -> ! {
    let id = task::switching::get_current_id();
    crate::kprintln!("INIT task: {:?}", id);
    // initialize drivers that rely on multitasking
    {
        console::init_console();

        let con = task::actions::handle::create_file_handle();
        task::actions::io::open_sync(con, "DEV:\\CON1").unwrap();

        hardware::ps2::install_drivers();

        system_log(con, "Installing ATA Drivers...\n");
        hardware::ata::install();

        system_log(con, "Installing Floppy Drivers...\n");
        hardware::floppy::install();

        system_log(con, "Installing Network Device Drivers...\n");
        hardware::ethernet::dev::install_driver();

        system_log(con, "Initializing Net Stack...\n");
        net::start_net_stack();

        system_log(con, "Mounting FAT FS...\n");
        io::filesystem::fatfs::mount_fat_fs();

        system_log(con, "System ready! Welcome to IDOS\n\n");
        console::console_ready();
    }

    {
        // Loader test
        let loader_id = loader::resident::get_loader_id();

        let (child_handle, child_id) = task::actions::handle::create_task();
        let child_lock = task::switching::get_task(child_id).unwrap();
        let mut child_guard = child_lock.write();
        child_guard.push_arg("apples");
        child_guard.push_arg("banana");
        child_guard.push_arg("cherry");

        loader::load_executable(child_id, "A:\\ELFTEST.ELF");
    }

    /*{
        // TCP test
        use net::socket::SocketPort;
        use net::ip::IPV4Address;

        let sock = net::socket::create_socket(net::socket::SocketProtocol::TCP);
        net::socket::bind_socket(sock, IPV4Address([127, 0, 0, 1]), SocketPort::new(84), IPV4Address([0, 0, 0, 0]), SocketPort::new(0)).unwrap();

        crate::kprintln!("Listening on 127.0.0.1:84");
        let connection = loop {
            match net::socket::socket_accept(sock) {
                Some(handle) => break handle,
                None => crate::task::actions::yield_coop(),
            }
        };
        crate::kprintln!("Accepted connection from remote endpoint");

        let mut buffer = alloc::vec::Vec::new();
        for _ in 0..1024 {
            buffer.push(0);
        }
        loop {
            if let Some(len) = net::socket::socket_read(connection, buffer.as_mut_slice()) {
                crate::kprintln!("GOT PAYLOAD");
                let s = core::str::from_utf8(&buffer[..len]).unwrap();
                crate::kprintln!("\"{}\"", s);
            }
            task::actions::yield_coop();
        }
    }*/

    let wake_set = task::actions::sync::create_wake_set();
    loop {
        task::actions::sync::block_on_wake_set(wake_set, None);
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
