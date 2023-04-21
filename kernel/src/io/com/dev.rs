//! Device Driver for COM Ports

use crate::filesystem::drivers::asyncfs::AsyncDriver;
use crate::task::actions::{read_message_blocking, send_message};
use crate::task::actions::lifecycle::{create_kernel_task, terminate};
use crate::task::id::TaskID;
use crate::task::messaging::Message;

use super::serial::SerialPort;

pub fn install_driver(_name: &str, base_port: u16) -> Result<TaskID, ()> {
    // right now this is getting called from within the fs initialization path
    // Once we add a global configure() method to all kernel fs, this can
    // instead tell the DEV: fs directly to install a new driver with a
    // specific name
    let task = create_kernel_task(run_driver);
    send_message(task, Message(base_port as u32, 0, 0, 0), 0xffffffff);

    Ok(task)
}

struct ComDeviceDriver {
    port: SerialPort,
}

impl ComDeviceDriver {
    pub fn new(port_no: u16) -> Self {
        let port = SerialPort::new(port_no);
        port.init();
        Self {
            port,
        }
    }
}

impl AsyncDriver for ComDeviceDriver {
    fn open(&mut self, path: &str) -> u32 {
        1
    }

    fn read(&mut self, buffer: &mut [u8]) -> u32 {
        for i in 0..buffer.len() {
            buffer[i] = b'A';
        }
        buffer.len() as u32
    }

    fn write(&mut self, buffer: &[u8]) -> u32 {
        0
    }

    fn close(&mut self, handle: u32) {
        
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
            loop {}
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

