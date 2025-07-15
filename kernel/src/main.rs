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
pub mod conman;
pub mod console;
pub mod dos;
pub mod executor;
pub mod files;
pub mod graphics;
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
pub mod random;
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

    let initial_pagedir = memory::virt::page_table::get_current_pagedir();
    let bsp_cpu_scheduler = task::switching::init(initial_pagedir);

    acpi::init();

    init::init_hardware();

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
            // Disable interrupts because task switching is not safe to interrupt
            asm!("cli");
            task::scheduling::switch();
            // When this is reached, it means the BSP has run out of available
            // work -- all available tasks are blocked.
            // Resuming interrupts and halting the CPU saves power until something
            // interesting happens.
            asm!("sti", "hlt",);
        }
    }
}

fn init_system() -> ! {
    let mut logger = log::BufferedLogger::new();

    let id = task::switching::get_current_id();
    crate::kprintln!("INIT task: {:?}", id);
    // initialize drivers that rely on multitasking
    {
        let _loader_id = loader::resident::get_loader_id();
        //let con = task::actions::handle::create_file_handle();
        //task::actions::io::open_sync(con, "DEV:\\CON1").unwrap();

        hardware::ps2::install_drivers();

        logger.log("Installing ATA Drivers...\n");
        hardware::ata::install();

        logger.log("Installing Floppy Drivers...\n");
        hardware::floppy::install();

        logger.log("Installing Network Device Drivers...\n");
        hardware::ethernet::dev::install_driver();

        logger.log("Initializing Net Stack...\n");
        net::start_net_stack();

        logger.log("Mounting FAT FS...\n");
        io::filesystem::fatfs::mount_fat_fs();

        logger.log("Initializing Graphics Driver...\n");
        graphics::register_graphics_driver("C:\\GFX.ELF");

        console::init_console();

        let con = task::actions::handle::create_file_handle();
        task::actions::io::open_sync(con, "DEV:\\CON1").unwrap();

        logger.log("\nSystem ready! Welcome to IDOS\n\n");
        logger.flush_to_file(con);
        console::console_ready();
    }

    {
        // tcp test 2
        let socket_handle = task::actions::handle::create_tcp_socket();
        let open_op = idos_api::io::AsyncOp {
            op_code: idos_api::io::ASYNC_OP_OPEN,
            return_value: core::sync::atomic::AtomicU32::new(0),
            signal: core::sync::atomic::AtomicU32::new(0),
            args: [0, 2020, 0],
        };
        let _ = task::actions::io::send_io_op(socket_handle, &open_op, None);
        while !open_op.is_complete() {
            task::actions::yield_coop();
        }
        crate::kprintln!("OPENED SOCKET");

        let mut read_buffer: [u8; 12] = [0; 12];
        let accept_op = idos_api::io::AsyncOp {
            op_code: idos_api::io::ASYNC_OP_READ,
            return_value: core::sync::atomic::AtomicU32::new(0),
            signal: core::sync::atomic::AtomicU32::new(0),
            args: [read_buffer.as_ptr() as u32, 0, 0],
        };
        let _ = task::actions::io::send_io_op(socket_handle, &accept_op, None);
        while !accept_op.is_complete() {
            task::actions::sleep(1000);
        }
        let conn_handle = io::handle::Handle::new(
            accept_op
                .return_value
                .load(core::sync::atomic::Ordering::SeqCst) as usize,
        );
        crate::kprintln!("Accept TCP connection");
        loop {
            let read_op = idos_api::io::AsyncOp {
                op_code: idos_api::io::ASYNC_OP_READ,
                return_value: core::sync::atomic::AtomicU32::new(0),
                signal: core::sync::atomic::AtomicU32::new(0),
                args: [read_buffer.as_ptr() as u32, 12, 0],
            };
            let _ = task::actions::io::send_io_op(conn_handle, &read_op, None);
            while !read_op.is_complete() {
                task::actions::sleep(1000);
            }
            let read_len = read_op
                .return_value
                .load(core::sync::atomic::Ordering::SeqCst) as usize;
            let read_str =
                core::str::from_utf8(&read_buffer[..read_len]).unwrap_or("Invalid UTF-8");
            crate::kprintln!("READ ({}): \"{}\"", read_len, read_str);
        }
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
