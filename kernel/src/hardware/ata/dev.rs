use alloc::vec::Vec;

use crate::collections::SlotList;
use crate::filesystem::drivers::asyncfs::AsyncDriver;
use crate::task::actions::lifecycle::{create_kernel_task, terminate};
use crate::task::actions::{read_message_blocking, send_message};
use crate::task::messaging::Message;
use crate::task::switching::get_current_id;
use crate::filesystem::install_device_driver;
use super::controller::{AtaController, DriveSelect};

struct AtaDeviceDriver {
    controller: AtaController,
    attached: Vec<AtaDrive>,
    open_handle_map: SlotList<OpenHandle>,
}

struct AtaDrive {
    location: DriveSelect,
}

impl AtaDeviceDriver {
    pub fn new(controller: AtaController) -> Self {
        Self {
            controller,
            attached: Vec::new(),
            open_handle_map: SlotList::new(),
        }
    }

    pub fn add_device(&mut self, location: DriveSelect) -> u32 {
        let drive = AtaDrive {
            location,
        };

        self.attached.push(drive);
        self.attached.len() as u32
    }
}

impl AsyncDriver for AtaDeviceDriver {
    fn open(&mut self, path: &str) -> u32 {
        crate::kprint!("ATA Open Path {}\n", path);
        // TODO: Parse path to determine which drive to open
        let handle = OpenHandle {
            drive: 0,
        };
        self.open_handle_map.insert(handle) as u32
    }

    fn read(&mut self, instance: u32, buffer: &mut [u8]) -> u32 {
        let drive_index = match self.open_handle_map.get(instance as usize) {
            Some(handle) => handle.drive,
            None => return 0, // handle doesn't exist
        };
        let location = match self.attached.get(drive_index) {
            Some(drive) => drive.location,
            None => return 0,
        };

        let mut pio_buffer: [u8; 512] = [0; 512];
        let first_sector = 0;
        self.controller.read_sectors(location, first_sector, &mut pio_buffer);
        for i in 0..buffer.len() {
            buffer[i] = pio_buffer[i];
        }
        buffer.len() as u32
    }

    fn write(&mut self, instance: u32, buffer: &[u8]) -> u32 {
        0
    }

    fn close(&mut self, handle: u32) {
        self.open_handle_map.remove(handle as usize);
    }
}

struct OpenHandle {
    drive: usize,
}

fn run_driver() -> ! {
    let (driver_no, base_port, control_port) = match read_message_blocking(None) {
        (Some(packet), _) => {
            let (_, message) = packet.open();
            (message.0, message.1 as u16, message.2 as u16)
        },
        (None, _) => {
            terminate(0);
        },
    };

    crate::kprint!("Install ATA device driver ({:#x}, {:#x})\n", base_port, control_port);

    let task_id = get_current_id();
    let mut ata_count = 0;

    let mut bus = AtaController::new(base_port, control_port);
    let disks = bus.identify();
    let mut driver_impl = AtaDeviceDriver::new(bus);
    for disk in disks {
        if let Some(info) = disk {
            ata_count += 1;
            crate::kprint!("    {}\n", info);
            let ata_index = driver_no * 2 + ata_count;
            let dev_name = alloc::format!("ATA{}", ata_index);
            crate::kprint!("Install driver as DEV:\\{}\n", dev_name);
            let sub_id = driver_impl.add_device(info.location);
            install_device_driver(dev_name.as_str(), task_id, sub_id);
        }
    }

    crate::kprint!("Detected {} ATA device(s)\n", ata_count);

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

pub fn install_drivers() {
    let configs = [
        (0x1f0, 0x3f6),
        (0x170, 0x176),
    ];

    let mut driver_no = 0;
    for (base_port, control_port) in configs {
        let task = create_kernel_task(run_driver);
        send_message(task, Message(driver_no, base_port, control_port, 0), 0xffffffff);
        driver_no += 1;
    }
}
