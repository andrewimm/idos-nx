//! Device Driver for COM Ports

use crate::collections::SlotList;
use crate::filesystem::drivers::asyncfs::AsyncDriver;
use crate::interrupts::pic::install_interrupt_handler;
use crate::task::actions::{read_message_blocking, send_message};
use crate::task::actions::lifecycle::{create_kernel_task, terminate, wait_for_io};
use crate::task::id::TaskID;
use crate::task::messaging::Message;
use crate::task::switching::get_task;
use spin::RwLock;
use super::serial::SerialPort;

static INSTALLED_DRIVERS: [RwLock<Option<TaskID>>; 2] = [
    RwLock::new(None),
    RwLock::new(None),
];

pub fn install_driver(_name: &str, base_port: u16) -> Result<TaskID, ()> {
    // right now this is getting called from within the fs initialization path
    // Once we add a global configure() method to all kernel fs, this can
    // instead tell the DEV: fs directly to install a new driver with a
    // specific name
    let task = create_kernel_task(run_driver);
    send_message(task, Message(base_port as u32, 0, 0, 0), 0xffffffff);

    match base_port {
        0x3f8 => {
            INSTALLED_DRIVERS[0].write().replace(task);
            install_interrupt_handler(4, com_interrupt_handler);
        },
        0x2f8 => {
            INSTALLED_DRIVERS[1].write().replace(task);
            install_interrupt_handler(3, com_interrupt_handler);
        },
        _ => (),
    }

    Ok(task)
}

pub fn com_interrupt_handler(irq: u32) {
    let index = match irq {
        3 => 1,
        4 => 0,
        _ => return,
    };
    let driver = INSTALLED_DRIVERS[index].read().clone();
    if let Some(task) = driver {
        // notify the driver
        let task_lock = get_task(task);
        if let Some(lock) = task_lock {
            lock.write().io_complete();
        }
    }
}

struct ComDeviceDriver {
    port: SerialPort,
    open_handles: SlotList<()>,
}

impl ComDeviceDriver {
    pub fn new(port_no: u16) -> Self {
        let port = SerialPort::new(port_no);
        port.init();
        Self {
            port,
            open_handles: SlotList::new(),
        }
    }
}

impl AsyncDriver for ComDeviceDriver {
    fn open(&mut self, _path: &str) -> u32 {
        let index = self.open_handles.insert(());
        index as u32
    }

    fn read(&mut self, handle: u32, buffer: &mut [u8]) -> u32 {
        if self.open_handles.get(handle as usize).is_none() {
            return 0;
        }
        let mut index = 0;
        while index < buffer.len() {
            while let Some(value) = self.port.read_byte() {
                buffer[index] = value;
                index += 1;
            }
            if index < buffer.len() {
                wait_for_io(None);
            }
        }
        buffer.len() as u32
    }

    fn write(&mut self, handle: u32, buffer: &[u8]) -> u32 {
        if self.open_handles.get(handle as usize).is_none() {
            return 0;
        }
        // TODO: make this not blocking
        let mut written = 0;
        for value in buffer.iter() {
            self.port.send_byte(*value);
            written += 1;
        }
        written
    }

    fn close(&mut self, handle: u32) {
        self.open_handles.remove(handle as usize);
    }
}

fn run_driver() -> ! {
    let port_no = match read_message_blocking(None) {
        (Some(packet), _) => {
            let (_, message) = packet.open();
            message.0 as u16
        },
        (None, _) => {
            terminate(0);
        },
    };

    crate::kprint!("Install COM device driver at port {:#X}\n", port_no);

    let mut driver_impl = ComDeviceDriver::new(port_no);

    loop {
        let (message_read, _) = read_message_blocking(None);
        if let Some(packet) = message_read {
            let (sender, message) = packet.open();

            match driver_impl.handle_request(message) {
                Some(response) => send_message(sender, response, 0xffffffff),
                None => continue,
            }
        }
    }
}

