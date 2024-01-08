#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(atomic_mut_ptr)]
#![feature(const_btree_new)]
#![feature(const_mut_refs)]
#![feature(custom_test_frameworks)]
#![feature(map_try_insert)]
#![feature(naked_functions)]
#![feature(vec_into_raw_parts)]

#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::arch::asm;

extern crate alloc;

pub mod arch;
pub mod cleanup;
pub mod collections;
pub mod command;
pub mod console;
pub mod devices;
pub mod dos;
pub mod files;
pub mod filesystem;
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

    let initial_pagedir = memory::virt::page_table::get_current_pagedir();
    task::switching::init(initial_pagedir);

    task::actions::lifecycle::create_kernel_task(cleanup::cleanup_task, Some("CLEANUP"));

    filesystem::init_fs();

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
        console::init_console();

        let con = task::actions::io::open_path("DEV:\\CON1").unwrap();

        hardware::ps2::install_drivers();

        task::actions::io::write_file(con, "Installing ATA Drivers...\n".as_bytes());
        hardware::ata::dev::install_drivers();

        task::actions::io::write_file(con, "Installing Floppy Drivers...\n".as_bytes());
        //hardware::floppy::dev::install_drivers();

        // new floppy driver
        hardware::floppy::install();

        task::actions::io::write_file(con, "Installing Network Device Drivers...\n".as_bytes());
        hardware::ethernet::dev::install_driver();

        task::actions::io::write_file(con, "Initializing Net Stack...\n".as_bytes());
        net::start_net_stack();

        {
            let mac = net::with_active_device(|dev| dev.mac).unwrap();
            task::actions::io::write_file(con,
                alloc::format!("Network device MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}\n", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]).as_bytes()
            );
            task::actions::io::write_file(con, "Resolving IP Address...\n".as_bytes());
            match net::get_active_device_ip(Some(2000)) {
                Some(ip) => {
                    task::actions::io::write_file(con, alloc::format!("Got IP: {:}\n", ip).as_bytes());
                },
                None => {
                    task::actions::io::write_file(con, "DHCP request timed out!\n".as_bytes());
                },
            }
        }

        task::actions::io::write_file(con, "Mounting FAT FS...\n".as_bytes());
        filesystem::drivers::fatfs::mount_fat_fs();

        task::actions::io::write_file(con, "System ready! Welcome to IDOS\n\n".as_bytes());
        console::console_ready();

        {
            // TODO: clean up this testing code
            let handle = task::actions::handle::create_file_handle();
            crate::task::actions::handle::handle_op_open(handle, "DEV:\\FD1").wait_for_completion();
            let mut buffer: [u8; 5] = [0; 5];
            let read = crate::task::actions::handle::handle_op_read(handle, &mut buffer).wait_for_completion();
            crate::kprint!("Read these {} bytes: ", read);
            for i in 0..buffer.len() {
                crate::kprint!("{:#X} ", buffer[i]);
            }
            crate::kprintln!();
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

    loop {
        task::actions::lifecycle::wait_for_io(None);
    }
}

fn task_a_body() -> ! {
    let mut buf: [u8; 5] = [b'A'; 5];
    let exec_child = task::actions::lifecycle::create_task();
    task::actions::lifecycle::attach_executable_to_task(exec_child, "A:\\TEST.BIN");
    task::actions::lifecycle::wait_for_child(exec_child, None);

    {
        use net::socket::SocketPort;

        let mac = net::with_active_device(|dev| dev.mac).unwrap();
        crate::kprintln!("Current MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
        crate::kprintln!("Resolve current IP");
        let current_ip = net::get_active_device_ip(Some(1000)).expect("DHCP request timed out!");
        crate::kprintln!("Got IP: {:}", current_ip);

        let socket = net::socket::create_socket(net::socket::SocketProtocol::UDP);
        net::socket::bind_socket(socket, current_ip, SocketPort::new(80), net::ip::IPV4Address([10, 0, 2, 3]), SocketPort::new(80));

        net::socket::socket_send(socket, &[11, 22, 33, 55]);
    }

    {
        crate::kprintln!("Read bytes from KERNEL");
        let kernel_file = task::actions::io::open_path("C:\\KERNEL.BIN").unwrap();
        task::actions::io::seek_file(kernel_file, files::cursor::SeekMethod::Absolute(0x11fe));
        let bytes_read = task::actions::io::read_file(kernel_file, &mut buf).unwrap();
        crate::kprint!("Read {} bytes: ", bytes_read);
        for i in 0..bytes_read {
            crate::kprint!("{:02X} ", buf[i as usize]);
        }
        crate::kprintln!("");
        task::actions::io::close_file(kernel_file);
    }

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

