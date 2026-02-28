#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(adt_const_params)]
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
pub mod config;
pub mod conman;
pub mod console;
pub mod exec;
pub mod executor;
pub mod files;
pub mod graphics;
pub mod hardware;
pub mod init;
pub mod interrupts;
pub mod io;
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

    // Bootstrap: these must be hardcoded because they're needed to read
    // the config file from C:\.
    hardware::ps2::install_drivers();

    logger.log("Installing ATA Drivers...\n");
    hardware::ata::install();

    logger.log("Mounting C:\\ ...\n");
    io::filesystem::fatfs::mount_fat_fs_single("C", "ATA1");

    // Read config file now that C:\ is available
    logger.log("Reading DRIVERS.CFG...\n");
    let directives = config::read_config("C:\\DRIVERS.CFG");

    if directives.is_empty() {
        logger.log("Warning: no directives found in DRIVERS.CFG, using defaults\n");
        // Fallback: run the old hardcoded sequence for everything after C:\ mount
        hardware::floppy::install();
        hardware::ethernet::dev::install_driver();
        net::start_net_stack();
        io::filesystem::fatfs::mount_fat_fs_single("A", "FD1");
        graphics::register_graphics_driver("C:\\GFX.ELF");
        console::init_console();
    } else {
        for directive in &directives {
            execute_directive(&mut logger, directive);
        }
    }

    let con = task::actions::handle::create_file_handle();
    task::actions::io::open_sync(con, "DEV:\\CON1", 0).unwrap();

    logger.log("\nSystem ready! Welcome to IDOS\n\n");
    logger.flush_to_file(con);
    console::console_ready();

    let wake_set = task::actions::sync::create_wake_set();
    loop {
        task::actions::sync::block_on_wake_set(wake_set, None);
    }
}

fn execute_directive(logger: &mut log::BufferedLogger, directive: &config::Directive) {
    use config::Directive;
    match directive {
        Directive::Driver(name) => {
            match name.as_str() {
                "ps2" => {
                    // PS2 is already installed in bootstrap, skip
                    logger.log("Driver ps2 already installed (bootstrap)\n");
                }
                "ata" => {
                    // ATA is already installed in bootstrap, skip
                    logger.log("Driver ata already installed (bootstrap)\n");
                }
                "floppy" => {
                    logger.log("Installing Floppy Drivers...\n");
                    hardware::floppy::install();
                }
                "ethernet" => {
                    logger.log("Installing Network Device Drivers...\n");
                    hardware::ethernet::dev::install_driver();
                }
                _ => {
                    logger.log("Unknown driver: ");
                    logger.log(name.as_str());
                    logger.log("\n");
                }
            }
        }
        Directive::Pci {
            vendor_id: _,
            device_id: _,
            path: _,
            busmaster: _,
        } => {
            // PCI userspace driver launching will be implemented when we have
            // a userspace ELF that uses the PCI syscalls
            logger.log("PCI directive: not yet implemented\n");
        }
        Directive::Mount {
            drive_letter,
            fs_type,
            device,
        } => {
            match fs_type.as_str() {
                "FAT" => {
                    logger.log("Mounting ");
                    logger.log(drive_letter.as_str());
                    logger.log(":\\ ...\n");
                    io::filesystem::fatfs::mount_fat_fs_single(
                        drive_letter.as_str(),
                        device.as_str(),
                    );
                }
                _ => {
                    logger.log("Unknown filesystem type: ");
                    logger.log(fs_type.as_str());
                    logger.log("\n");
                }
            }
        }
        Directive::Graphics(path) => {
            logger.log("Initializing Graphics Driver...\n");
            graphics::register_graphics_driver(path.as_str());
        }
        Directive::Console => {
            console::init_console();
        }
        Directive::Net => {
            logger.log("Initializing Net Stack...\n");
            net::start_net_stack();
        }
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
