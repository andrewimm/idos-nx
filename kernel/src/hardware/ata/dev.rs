use alloc::vec::Vec;
use crate::collections::SlotList;
use crate::files::cursor::SeekMethod;
use crate::filesystem::drivers::asyncfs::AsyncDriver;
use crate::task::actions::io::{transfer_handle, open_pipe, read_file, write_file};
use crate::task::actions::lifecycle::{create_kernel_task, terminate};
use crate::task::actions::{read_message_blocking, send_message};
use crate::task::files::FileHandle;
use crate::task::messaging::Message;
use crate::task::switching::get_current_id;
use crate::filesystem::install_device_driver;
use super::controller::{AtaController, DriveSelect, SECTOR_SIZE};

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
            position: 0,
        };
        self.open_handle_map.insert(handle) as u32
    }

    fn read(&mut self, instance: u32, buffer: &mut [u8]) -> u32 {
        let (drive_index, position) = match self.open_handle_map.get(instance as usize) {
            Some(handle) => (handle.drive, handle.position),
            None => return 0, // handle doesn't exist
        };
        let location = match self.attached.get(drive_index) {
            Some(drive) => drive.location,
            None => return 0,
        };

        if position % SECTOR_SIZE == 0 && buffer.len() % SECTOR_SIZE == 0 {
            // if the read is sector aligned, we can optimize by writing direct
            // from the disk to the buffer
            let first_sector = (position / SECTOR_SIZE) as u32;
            let sectors_read = self.controller.read_sectors(location, first_sector, buffer).unwrap();
            let bytes_read = sectors_read as usize * SECTOR_SIZE;

            self.open_handle_map.get_mut(instance as usize).unwrap().position += bytes_read;
            return bytes_read as u32;
        }

        // unoptimized flow, using an intermediate buffer
        let mut bytes_read = 0;
        let mut pio_buffer: [u8; 512] = [0; 512];

        while bytes_read < buffer.len() {
            let read_position = position + bytes_read;
            let sector_index = read_position / SECTOR_SIZE;
            let sector_offset = read_position % SECTOR_SIZE;
            let bytes_remaining_in_sector = SECTOR_SIZE - sector_offset;
            let bytes_remaining_in_buffer = buffer.len() - bytes_read;

            self.controller.read_sectors(location, sector_index as u32, &mut pio_buffer);

            let bytes_to_copy = bytes_remaining_in_sector.min(bytes_remaining_in_buffer);

            for i in 0..bytes_to_copy {
                let to = bytes_read + i;
                let from = sector_offset + i;

                buffer[to] = pio_buffer[from];
            }

            bytes_read += bytes_to_copy;
        }

        self.open_handle_map.get_mut(instance as usize).unwrap().position += bytes_read;
        return bytes_read as u32;
    }

    fn write(&mut self, instance: u32, buffer: &[u8]) -> u32 {
        0
    }

    fn close(&mut self, handle: u32) {
        self.open_handle_map.remove(handle as usize);
    }

    fn seek(&mut self, instance: u32, offset: SeekMethod) -> u32 {
        let current_position = match self.open_handle_map.get(instance as usize) {
            Some(handle) => handle.position,
            None => return 0,
        };
        let new_position = offset.from_current_position(current_position);
        self.open_handle_map.get_mut(instance as usize).unwrap().position = new_position;
        new_position as u32
    }
}

struct OpenHandle {
    drive: usize,
    position: usize,
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

    let task_id = get_current_id();

    crate::kprint!("Install ATA device driver ({:#x}, {:#x}) {:?}\n", base_port, control_port, task_id);

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

    write_file(FileHandle::new(0), &[1]);

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
        let (pipe_read, pipe_write) = open_pipe().unwrap();
        let task = create_kernel_task(run_driver);
        transfer_handle(pipe_write, task).unwrap();
        send_message(task, Message(driver_no, base_port, control_port, 0), 0xffffffff);
        read_file(pipe_read, &mut [0u8]).unwrap();
        driver_no += 1;
    }
}
