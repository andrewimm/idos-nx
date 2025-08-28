use core::sync::atomic::{AtomicU32, Ordering};

use crate::{
    interrupts::pic::install_interrupt_handler, memory::address::VirtualAddress,
    sync::futex::futex_wake, task::actions::lifecycle::create_kernel_task,
};

pub mod controller;
pub mod driver;
pub mod keyboard;
pub mod keycodes;
pub mod mouse;

static DRIVER_ID: AtomicU32 = AtomicU32::new(0);

pub fn install_drivers() {
    crate::kprint!("Initialize PS/2\n");
    // TODO: When ACPI is enabled, use the data to confirm that PS/2 exists

    let _device_ready = self::controller::initialize_controller();

    if self::controller::reset_device() {
        // initialize keyboard
        crate::kprint!("PS/2 Keyboard reset, ready\n");
        install_interrupt_handler(1, kbd_handler, None);
    }

    self::controller::send_ps2_command(0xd4);
    if self::controller::reset_device() {
        // initialize mouse

        // set sample rate
        self::controller::send_ps2_command(0xd4);
        self::controller::write_ps2_data(0xf3);
        while !self::controller::data_read_ready() {}
        self::controller::read_ps2_data();
        self::controller::send_ps2_command(0xd4);
        self::controller::write_ps2_data(100);
        while !self::controller::data_read_ready() {}
        self::controller::read_ps2_data();

        // set resolution
        self::controller::send_ps2_command(0xd4);
        self::controller::write_ps2_data(0xe8);
        while !self::controller::data_read_ready() {}
        self::controller::read_ps2_data();
        self::controller::send_ps2_command(0xd4);
        self::controller::write_ps2_data(3); // 4 pixels per count
        while !self::controller::data_read_ready() {}
        self::controller::read_ps2_data();

        // enable streaming
        self::controller::send_ps2_command(0xd4);
        self::controller::write_ps2_data(0xf4);
        while !self::controller::data_read_ready() {}
        self::controller::read_ps2_data();

        install_interrupt_handler(12, mouse_handler, None);
    }

    let task_id = create_kernel_task(self::driver::ps2_driver_task, Some("PS2DEV"));
    DRIVER_ID.store(task_id.into(), Ordering::SeqCst);

    crate::kprint!("PS/2 set up complete.\n");
}

fn kbd_handler(_: u32) {
    let data = self::controller::read_ps2_data();
    if !self::driver::KEYBOARD_BUFFER.write(data) {
        crate::kprintln!("Keyboard overflow");
    }
    driver::DATA_READY.fetch_add(1, Ordering::SeqCst);
    futex_wake(VirtualAddress::new(driver::DATA_READY.as_ptr() as u32), 1);
}

fn mouse_handler(_: u32) {
    while self::controller::data_read_ready() {
        let data = self::controller::read_ps2_data();
        if !self::driver::MOUSE_BUFFER.write(data) {
            crate::kprintln!("Mouse overflow");
            break;
        }
    }
    driver::DATA_READY.fetch_add(1, Ordering::SeqCst);
    futex_wake(VirtualAddress::new(driver::DATA_READY.as_ptr() as u32), 1);
}
