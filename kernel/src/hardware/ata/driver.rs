use super::controller::{AtaController, DriveSelect, SECTOR_SIZE};
use crate::io::driver::async_driver::AsyncDriver;
use crate::io::driver::comms::IOResult;
use crate::io::filesystem::install_task_dev;
use crate::io::handle::Handle;
use crate::io::IOError;
use crate::task::actions::handle::open_message_queue;
use crate::task::actions::io::driver_io_complete;
use crate::task::actions::io::read_struct_sync;
use crate::task::actions::io::write_sync;
use crate::task::messaging::Message;
use crate::task::switching::get_current_id;
use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU32, Ordering};

pub struct AtaDeviceDriver {
    controller: AtaController,

    /// Each bus can have up to two attached ATA devices. When the driver is
    /// initialized, it detects these drives and stores access info in the
    /// `attached` array.
    pub attached: [Option<DriveSelect>; 2],

    next_instance: AtomicU32,
    open_instances: BTreeMap<u32, DriveSelect>,
}

impl AtaDeviceDriver {
    pub fn new(bus: AtaController) -> Self {
        Self {
            controller: bus,
            attached: [None, None],
            next_instance: AtomicU32::new(1),
            open_instances: BTreeMap::new(),
        }
    }
}

impl AsyncDriver for AtaDeviceDriver {
    fn open(&mut self, path: &str) -> IOResult {
        // The `path` should be a stringified version of the driver index.
        // The driver number is 1-indexed, while the internal array is
        // 0-indexed.
        crate::kprintln!("ATA Open Path {}", path);
        let attached_index = match path.parse::<usize>() {
            Ok(i) => i - 1,
            Err(_) => return Err(IOError::NotFound),
        };
        if attached_index >= self.attached.len() {
            return Err(IOError::NotFound);
        }
        if let Some(select) = self.attached[attached_index] {
            let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
            self.open_instances.insert(instance, select);
            return Ok(instance);
        }
        Err(IOError::NotFound)
    }

    fn close(&mut self, instance: u32) -> IOResult {
        self.open_instances
            .remove(&instance)
            .map(|_| 1)
            .ok_or(IOError::FileHandleInvalid)
    }

    fn read(&mut self, instance: u32, buffer: &mut [u8], offset: u32) -> IOResult {
        let select = self
            .open_instances
            .get(&instance)
            .cloned()
            .ok_or(IOError::FileHandleInvalid)?;

        // If the read is sector-aligned, we can DMA transfer directly into
        // the destination buffer.
        if offset % SECTOR_SIZE as u32 == 0 && buffer.len() % SECTOR_SIZE == 0 {
            let first_sector = offset / SECTOR_SIZE as u32;
            let sectors_read = self
                .controller
                .read_sectors(select, first_sector, buffer)
                .map_err(|_| IOError::FileSystemError)?;
            let bytes_read = sectors_read * SECTOR_SIZE as u32;
            return Ok(bytes_read);
        }

        // unoptimized flow using an intermediate buffer
        let mut bytes_read: u32 = 0;
        let mut pio_buffer: [u8; 512] = [0; 512];

        while bytes_read < buffer.len() as u32 {
            let read_position: u32 = offset + bytes_read;
            let sector_index: u32 = read_position / SECTOR_SIZE as u32;
            let sector_offset: u32 = read_position % SECTOR_SIZE as u32;
            let bytes_remaining_in_sector: u32 = SECTOR_SIZE as u32 - sector_offset;
            let bytes_remaining_in_buffer: u32 = buffer.len() as u32 - bytes_read;

            self.controller
                .read_sectors(select, sector_index, &mut pio_buffer)
                .map_err(|_| IOError::FileSystemError)?;

            let bytes_to_copy = bytes_remaining_in_sector.min(bytes_remaining_in_buffer);

            for i in 0..bytes_to_copy {
                let to = (bytes_read + i) as usize;
                let from = (sector_offset + i) as usize;

                buffer[to] = pio_buffer[from];
            }

            bytes_read += bytes_to_copy;
        }

        Ok(bytes_read)
    }
}

/// This task is designed to be run once for each ATA controller. Controllers
/// may have one or two drives attached. Since commands can only be issued to
/// one drive at a time, it does not make sense to multitask the two drives.
/// The task will read three 16-bit numbers from stdin:
///   The unique index of the ATA controller, used for numbering device names
///   The base port for the ATA controller
///   The control port for the ATA controller
pub fn run_driver() -> ! {
    let task_id = get_current_id();

    let stdin = Handle::new(0);
    let mut args: [u16; 3] = [0; 3];

    let _ = read_struct_sync(stdin, &mut args, 0);
    let driver_no = args[0];
    let base_port = args[1];
    let control_port = args[2];

    crate::kprintln!(
        "Install ATA device driver ({:#x}, {:#x}) {:?}",
        base_port,
        control_port,
        task_id
    );

    let mut ata_count = 0;

    let bus = AtaController::new(base_port, control_port);
    let disks = bus.identify();
    let mut driver_impl = AtaDeviceDriver::new(bus);
    let mut device_count = 0;
    for disk in disks {
        if let Some(info) = disk {
            ata_count += 1;
            crate::kprintln!("    {}", info);
            let ata_index = driver_no * 2 + ata_count;
            let dev_name = alloc::format!("ATA{}", ata_index);
            crate::kprintln!("Install driver as DEV:\\{}", dev_name);
            driver_impl.attached[device_count] = Some(info.location);
            device_count += 1;
            install_task_dev(dev_name.as_str(), task_id, device_count as u32);
        }
    }

    crate::kprintln!("Detected {} ATA device(s)", ata_count);

    let _ = write_sync(Handle::new(1), &[1], 0);

    // prepare message event loop
    let messages = open_message_queue();
    let mut incoming_message = Message::empty();

    loop {
        if let Ok(_sender) = read_struct_sync(messages, &mut incoming_message, 0) {
            let request_id = incoming_message.unique_id;
            match driver_impl.handle_request(incoming_message) {
                Some(result) => driver_io_complete(request_id, result),
                None => (),
            }
        }
    }
}
