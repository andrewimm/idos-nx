//! Async IO-compatible device driver for COM ports
//!
//! The COM driver handles incoming data from the port, as well as data written
//! by user programs that should be output on the port.

use crate::{
    task::actions::{
        handle::{
            open_message_queue,
            open_interrupt_handle,
            create_notify_queue,
            add_handle_to_notify_queue,
            wait_on_notify,
        },
        lifecycle::create_kernel_task,
    },
    io::{
        handle::PendingHandleOp,
        async_io::{
            OPERATION_FLAG_INTERRUPT,
            INTERRUPT_OP_LISTEN,
            INTERRUPT_OP_ACK,
        },
    },
};

use super::serial::SerialPort;

/// Main event loop of the COM driver
pub fn run_driver() -> ! {
    let messages = open_message_queue();

    let interrupt = open_interrupt_handle(4);

    // notify queue waits on both the hardware interrupt and messages from the
    // filesystem to the driver. Either of these signals will wake the main
    // loop.
    let notify = create_notify_queue();
    add_handle_to_notify_queue(notify, messages);
    add_handle_to_notify_queue(notify, interrupt);

    let serial = SerialPort::new(0x3f8);
    serial.init();

    loop {
        let interrupt_read = PendingHandleOp::new(interrupt, OPERATION_FLAG_INTERRUPT | INTERRUPT_OP_LISTEN, 0, 0, 0);
        if !interrupt_read.is_complete() {
            wait_on_notify(notify, None);
        }
        crate::kprintln!("COM GOT AN INTERRUPT");
        PendingHandleOp::new(interrupt, OPERATION_FLAG_INTERRUPT | INTERRUPT_OP_ACK, 0, 0, 0);

        loop {
            match serial.read_byte() {
                Some(b) => (),
                None => break,
            }
        }
    }
}

pub fn install() {
    create_kernel_task(run_driver, Some("COMDEV"));
}
