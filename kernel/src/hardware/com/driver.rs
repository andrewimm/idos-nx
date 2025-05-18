//! Async IO-compatible device driver for COM ports
//!
//! The COM driver handles incoming data from the port, as well as data written
//! by user programs that should be output on the port.

use core::sync::atomic::{AtomicU32, Ordering};

use alloc::collections::{BTreeMap, VecDeque};
use idos_api::io::{error::IOError, AsyncOp, ASYNC_OP_READ};

use crate::{
    io::driver::comms::{DriverCommand, IOResult, DRIVER_RESPONSE_MAGIC},
    task::{
        actions::{
            handle::{open_interrupt_handle, open_message_queue},
            io::{append_io_op, write_sync},
            lifecycle::create_kernel_task,
            send_message,
            sync::{block_on_wake_set, create_wake_set},
        },
        id::TaskID,
        messaging::Message,
    },
};

use super::serial::SerialPort;

/// Main event loop of the COM driver
pub fn run_driver() -> ! {
    let messages_handle = open_message_queue();
    let mut incoming_message = Message::empty();

    let irq_handle = open_interrupt_handle(4);
    let mut interrupt_ready: [u8; 1] = [0];

    let wake_set = create_wake_set();

    let mut driver_impl = ComDeviceDriver::new(0x3f8);

    let mut interrupt_read = AsyncOp::new(ASYNC_OP_READ, interrupt_ready.as_ptr() as u32, 1, 0);
    let _ = append_io_op(irq_handle, &interrupt_read, Some(wake_set));
    let mut message_read = AsyncOp::new(
        ASYNC_OP_READ,
        &mut incoming_message as *mut Message as u32,
        core::mem::size_of::<Message>() as u32,
        0,
    );
    let _ = append_io_op(messages_handle, &message_read, Some(wake_set));
    loop {
        if interrupt_read.is_complete() {
            let _ = write_sync(irq_handle, &[1], 0);

            driver_impl.init_read();

            interrupt_read = AsyncOp::new(ASYNC_OP_READ, interrupt_ready.as_ptr() as u32, 1, 0);
            let _ = append_io_op(irq_handle, &interrupt_read, Some(wake_set));
        } else if message_read.is_complete() {
            let sender = message_read.return_value.load(Ordering::SeqCst);
            driver_impl.handle_request(incoming_message, TaskID::new(sender));

            message_read = AsyncOp::new(
                ASYNC_OP_READ,
                &mut incoming_message as *mut Message as u32,
                core::mem::size_of::<Message>() as u32,
                0,
            );
            let _ = append_io_op(messages_handle, &message_read, Some(wake_set));
        } else {
            block_on_wake_set(wake_set, None);
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
            }
            DriverCommand::Read => {
                let buffer_ptr = message.args[1] as *mut u8;
                let buffer_len = message.args[2] as usize;
                self.read_list.push_back(PendingRead {
                    request_id: message.unique_id,
                    respond_to: sender,
                    buffer_ptr,
                    buffer_len,
                    written: 0,
                });
                if self.read_list.len() == 1 {
                    self.init_read();
                }
            }
            DriverCommand::Write => {
                let buffer_ptr = message.args[1] as *mut u8;
                let buffer_len = message.args[2] as usize;
                for i in 0..buffer_len {
                    unsafe {
                        self.serial.send_byte(*buffer_ptr.add(i));
                    }
                }
                self.send_response(sender, message.unique_id, Ok(buffer_len as u32));
            }
            _ => self.send_response(
                sender,
                message.unique_id,
                Err(IOError::UnsupportedOperation),
            ),
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
            }
            Err(err) => {
                let code = Into::<u32>::into(err) | 0x80000000;
                Message {
                    message_type: DRIVER_RESPONSE_MAGIC,
                    unique_id: request_id,
                    args: [code, 0, 0, 0, 0, 0],
                }
            }
        };
        send_message(task, message, 0xffffffff);
    }

    fn init_read(&mut self) {
        let first = match self.read_list.get_mut(0) {
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
                }
                None => break,
            }
        }
        if first.written < first.buffer_len {
            return;
        }
        let completed = self.read_list.pop_front().unwrap();
        self.send_response(
            completed.respond_to,
            completed.request_id,
            Ok(completed.written as u32),
        );
    }
}
