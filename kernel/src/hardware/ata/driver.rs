use super::controller::{AtaChannel, DriveSelect, SECTOR_SIZE};
use crate::io::filesystem::install_task_dev;
use crate::io::handle::Handle;
use crate::task::actions::handle::open_interrupt_handle;
use crate::task::actions::handle::open_message_queue;
use crate::task::actions::io::close_sync;
use crate::task::actions::io::driver_io_complete;
use crate::task::actions::io::read_struct_sync;
use crate::task::actions::io::read_sync;
use crate::task::actions::io::write_sync;
use crate::task::switching::get_current_id;
use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU32, Ordering};
use idos_api::io::driver::AsyncDriver;
use idos_api::io::driver::DriverFileReference;
use idos_api::io::error::{IoError, IoResult};
use idos_api::ipc::Message;

pub struct AtaDeviceDriver {
    channel: AtaChannel,

    /// Each bus can have up to two attached ATA devices. When the driver is
    /// initialized, it detects these drives and stores access info in the
    /// `attached` array.
    pub attached: [Option<DriveSelect>; 2],

    next_instance: AtomicU32,
    open_instances: BTreeMap<u32, DriveSelect>,
}

impl AtaDeviceDriver {
    pub fn new(channel: AtaChannel) -> Self {
        Self {
            channel,
            attached: [None, None],
            next_instance: AtomicU32::new(1),
            open_instances: BTreeMap::new(),
        }
    }
}

impl AsyncDriver for AtaDeviceDriver {
    fn release_buffer(&mut self, buffer_ptr: *mut u8, buffer_len: usize) {
        use crate::memory::{address::VirtualAddress, shared::release_buffer};
        release_buffer(VirtualAddress::new(buffer_ptr as u32), buffer_len);
    }

    fn open(&mut self, path: &str) -> IoResult<DriverFileReference> {
        // The `path` should be a stringified version of the driver index.
        // The driver number is 1-indexed, while the internal array is
        // 0-indexed.
        super::LOGGER.log(format_args!("Open path \"{}\"", path));
        let attached_index = match path.parse::<usize>() {
            Ok(i) => i - 1,
            Err(_) => return Err(IoError::NotFound),
        };
        if attached_index >= self.attached.len() {
            return Err(IoError::NotFound);
        }
        if let Some(select) = self.attached[attached_index] {
            let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
            self.open_instances.insert(instance, select);
            return Ok(DriverFileReference::new(instance));
        }
        Err(IoError::NotFound)
    }

    fn close(&mut self, file_ref: DriverFileReference) -> IoResult {
        self.open_instances
            .remove(&*file_ref)
            .map(|_| 1)
            .ok_or(IoError::FileHandleInvalid)
    }

    fn read(&mut self, file_ref: DriverFileReference, buffer: &mut [u8], offset: u32) -> IoResult {
        let select = self
            .open_instances
            .get(&*file_ref)
            .cloned()
            .ok_or(IoError::FileHandleInvalid)?;

        // If the read is sector-aligned, we can DMA transfer directly into
        // the destination buffer.
        if offset % SECTOR_SIZE as u32 == 0 && buffer.len() % SECTOR_SIZE == 0 {
            let first_sector = offset / SECTOR_SIZE as u32;
            let sectors_read = self
                .channel
                .read(select, first_sector, buffer)
                .map_err(|_| IoError::FileSystemError)?;
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

            self.channel
                .read(select, sector_index, &mut pio_buffer)
                .map_err(|_| IoError::FileSystemError)?;

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
/// The task will read the unique device number from stdin, followed by three
/// bytes that describe the PCI bus location.
pub fn run_driver() -> ! {
    let task_id = get_current_id();

    let args_reader = Handle::new(0);
    let response_writer = Handle::new(1);

    let mut drive_args: [u8; 1] = [0; 1];
    let mut port_args: [u16; 3] = [0; 3];
    let mut irq_args: [u8; 1] = [0; 1];

    let _ = read_sync(args_reader, &mut drive_args, 0);
    let _ = read_struct_sync(args_reader, &mut port_args, 0);
    let _ = read_sync(args_reader, &mut irq_args, 0);

    super::LOGGER.log(format_args!(
        "Install driver ({:X} {:X} {:X})",
        port_args[0], port_args[1], port_args[2]
    ));

    // access for primary channel
    let channel = AtaChannel {
        base_port: port_args[0],
        control_port: port_args[1],
        bus_master_port: if port_args[2] != 0 {
            Some(port_args[2])
        } else {
            None
        },
        irq_handle: Some(open_interrupt_handle(irq_args[0])),
    };
    let mut device_count = 0;

    let disks = channel.identify();
    let mut driver_impl = AtaDeviceDriver::new(channel);
    for disk in disks {
        if let Some(info) = disk {
            super::LOGGER.log(format_args!("    {}", info));
            driver_impl.attached[device_count] = Some(info.location);
            let drive_number = drive_args[0] + device_count as u8 + 1;
            let dev_name = alloc::format!("ATA{}", drive_number);
            super::LOGGER.log(format_args!("Installed driver as DEV:\\{}", dev_name));
            device_count += 1;
            install_task_dev(dev_name.as_str(), task_id, device_count as u32);
        }
    }

    let _ = write_sync(response_writer, &[device_count as u8], 0);
    let _ = close_sync(response_writer);

    if device_count == 0 {
        super::LOGGER.log(format_args!("No devices found"));
        crate::task::actions::lifecycle::terminate(0);
    }
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
