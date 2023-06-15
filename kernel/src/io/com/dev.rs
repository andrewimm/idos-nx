//! Device Driver for COM Ports

use crate::collections::SlotList;
use crate::files::cursor::SeekMethod;
use crate::files::error::IOError;
use crate::filesystem::drivers::asyncfs::AsyncDriver;
use crate::filesystem::install_device_driver;
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

pub fn install_drivers() {
    let configs = [
        (0x3f8, 4),
        (0x2f8, 3),
    ];

    for (index, (port, irq)) in configs.iter().enumerate() {
        let task = create_kernel_task(run_driver);
        send_message(task, Message(*port, 0, 0, 0), 0xffffffff);
        INSTALLED_DRIVERS[index].write().replace(task);
        install_interrupt_handler(*irq, com_interrupt_handler);
        let name = alloc::format!("COM{}", index + 1);

        install_device_driver(name.as_str(), task, 0).expect("Failed to install COM driver");
    }
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
    fn open(&mut self, _path: &str) -> Result<u32, IOError> {
        let index = self.open_handles.insert(());
        Ok(index as u32)
    }

    fn read(&mut self, handle: u32, buffer: &mut [u8]) -> Result<u32, IOError> {
        if self.open_handles.get(handle as usize).is_none() {
            return Err(IOError::FileHandleInvalid);
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
        Ok(buffer.len() as u32)
    }

    fn write(&mut self, handle: u32, buffer: &[u8]) -> Result<u32, IOError> {
        if self.open_handles.get(handle as usize).is_none() {
            return Err(IOError::FileHandleInvalid);
        }
        // TODO: make this not blocking
        let mut written = 0;
        for value in buffer.iter() {
            self.port.send_byte(*value);
            written += 1;
        }
        Ok(written)
    }

    fn close(&mut self, handle: u32) -> Result<(), IOError> {
        if self.open_handles.remove(handle as usize).is_some() {
            Ok(())
        } else {
            Err(IOError::FileHandleInvalid)
        }
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

