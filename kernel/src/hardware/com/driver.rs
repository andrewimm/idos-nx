//! Async IO-compatible device driver for COM ports
//!
//! The COM driver handles incoming data from the port, as well as data written
//! by user programs that should be output on the port.

use core::{sync::atomic::{AtomicU32, Ordering}, fmt::Write};

use alloc::collections::{BTreeMap, VecDeque};
use idos_api::io::error::IOError;

use crate::{
    task::{actions::{
        handle::{
            open_message_queue,
            open_interrupt_handle,
            create_notify_queue,
            add_handle_to_notify_queue,
            wait_on_notify, handle_op_read, handle_op_read_struct, handle_op_write,
        },
        lifecycle::create_kernel_task, send_message,
    }, messaging::Message, id::TaskID},
    io::{
        handle::PendingHandleOp,
        async_io::{
            OPERATION_FLAG_INTERRUPT,
            OPERATION_FLAG_MESSAGE,
        }, driver::comms::{IOResult, DriverCommand, DRIVER_RESPONSE_MAGIC},
    },
};

use super::serial::SerialPort;

/// Main event loop of the COM driver
pub fn run_driver() -> ! {
    let messages = open_message_queue();
    let mut incoming_message = Message::empty();

    let interrupt = open_interrupt_handle(4);
    let mut interrupt_ready: [u8; 1] = [0];

    // notify queue waits on both the hardware interrupt and messages from the
    // filesystem to the driver. Either of these signals will wake the main
    // loop.
    let notify = create_notify_queue();
    add_handle_to_notify_queue(notify, messages);
    add_handle_to_notify_queue(notify, interrupt);

    let mut driver_impl = ComDeviceDriver::new(0x3f8);

    let mut interrupt_read = handle_op_read(interrupt, &mut interrupt_ready);
    let mut message_read = handle_op_read_struct(messages, &mut incoming_message);
    loop {
        if interrupt_read.is_complete() {
            handle_op_write(interrupt, &[1]);

            driver_impl.init_read();

            interrupt_read = handle_op_read(interrupt, &mut interrupt_ready);
        } else if let Some(sender) = message_read.get_result() {
            driver_impl.handle_request(incoming_message, TaskID::new(sender));

            message_read = handle_op_read_struct(messages, &mut incoming_message);
        } else {
            wait_on_notify(notify, None);
        }
    }
}

pub fn install() {
    let task_id = create_kernel_task(run_driver, Some("COMDEV"));

    crate::io::filesystem::install_task_dev("COM1", task_id, 0);
}

struct ComDeviceDriver {
    serial: SerialPort,
    next_instance: AtomicU32,
    open_instances: BTreeMap<u32, OpenFile>,

    read_list: VecDeque<PendingRead>,
}

struct OpenFile {}

struct PendingRead {
    request_id: u32,
    respond_to: TaskID,
    buffer_ptr: *mut u8,
    buffer_len: usize,
    written: usize,
}

impl ComDeviceDriver {
    pub fn new(port: u16) -> Self {
        let serial = SerialPort::new(port);
        serial.init();

        Self {
            serial,
            next_instance: AtomicU32::new(1),
            open_instances: BTreeMap::new(),
            read_list: VecDeque::new(),
        }
    }

    pub fn handle_request(&mut self, message: Message, sender: TaskID) {
        match DriverCommand::from_u32(message.message_type) {
            DriverCommand::OpenRaw => {
                let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
                self.open_instances.insert(instance, OpenFile {});
                self.send_response(sender, message.unique_id, Ok(instance));
            },
            DriverCommand::Read => {
                let instance = message.args[0];
                let buffer_ptr = message.args[1] as *mut u8;
                let buffer_len = message.args[2] as usize;
                self.read_list.push_back(
                    PendingRead {
                        request_id: message.unique_id,
                        respond_to: sender,
                        buffer_ptr,
                        buffer_len,
                        written: 0,
                    }
                );
                if self.read_list.len() == 1 {
                    self.init_read();
                }
            },
            DriverCommand::Write => {
                let instance = message.args[0];
                let buffer_ptr = message.args[1] as *mut u8;
                let buffer_len = message.args[2] as usize;
                for i in 0..buffer_len {
                    unsafe {
                        self.serial.send_byte(*buffer_ptr.add(i));
                    }
                }
                self.send_response(sender, message.unique_id, Ok(buffer_len as u32));
            },
            _ => self.send_response(sender, message.unique_id, Err(IOError::UnsupportedOperation)),
        }
    }

    fn send_response(&self, task: TaskID, request_id: u32, result: IOResult) {
        let message = match result {
            Ok(result) => {
                let code = result & 0x7fffffff;
                Message {
                    message_type: DRIVER_RESPONSE_MAGIC,
                    unique_id: request_id,
                    args: [code, 0, 0, 0, 0, 0],
                }
            },
            Err(err) => {
                let code = Into::<u32>::into(err) | 0x80000000;
                Message {
                    message_type: DRIVER_RESPONSE_MAGIC,
                    unique_id: request_id,
                    args: [code, 0, 0, 0, 0, 0],
                }
            },
        };
        send_message(task, message, 0xffffffff);
    }

    fn init_read(&mut self) {
        let mut first = match self.read_list.get_mut(0) {
            Some(pending) => pending,
            None => return,
        };
        while first.written < first.buffer_len {
            match self.serial.read_byte() {
                Some(byte) => {
                    unsafe {
                        let ptr = first.buffer_ptr.add(first.written);
                        core::ptr::write_volatile(ptr, byte);
                    }
                    first.written += 1;
                },
                None => break,
            }
        }
        if first.written < first.buffer_len {
            return;
        }
        let completed = self.read_list.pop_front().unwrap();
        self.send_response(completed.respond_to, completed.request_id, Ok(completed.written as u32));
    }
}

