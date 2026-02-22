#![no_std]
#![no_main]

extern crate alloc;
extern crate idos_sdk;

// Place a trampoline at offset 0 in the flat binary so the kernel can jump
// directly to the load address. The SDK's _start (which sets up the allocator
// and calls main()) lives somewhere in .text; this stub just forwards to it.
core::arch::global_asm!(
    r#"
.section .entry, "ax"
.global _flat_entry
_flat_entry:
    jmp _start
"#
);

use fatdriver::disk::DiskIO;
use fatdriver::driver::{FatDriver, FatError, FileTypeInfo};

use idos_api::io::driver::{AsyncDriver, DriverFileReference, DriverMappingToken};
use idos_api::io::error::{IoError, IoResult};
use idos_api::io::file::{FileStatus, FileType};
use idos_api::io::sync::{close_sync, io_sync, open_sync, read_sync, write_sync};
use idos_api::io::Handle;
use idos_sdk::log::SysLogger;
use idos_api::ipc::Message;
use idos_api::syscall::io::{create_file_handle, create_message_queue_handle, driver_io_complete, register_fs};
use idos_api::syscall::time::get_system_time;

/// DiskIO implementation backed by IDOS block device syscalls
struct IdosDiskIO {
    mount_handle: Handle,
}

impl IdosDiskIO {
    fn new(device_path: &str) -> Self {
        let mount_handle = create_file_handle();
        open_sync(mount_handle, device_path, 0).unwrap();
        Self { mount_handle }
    }
}

impl DiskIO for IdosDiskIO {
    fn read(&mut self, buffer: &mut [u8], offset: u32) -> u32 {
        read_sync(self.mount_handle, buffer, offset).unwrap_or(0)
    }

    fn write(&mut self, buffer: &[u8], offset: u32) {
        // write_sync takes &[u8] but idos_api wants &[u8], this is fine
        let _ = write_sync(self.mount_handle, buffer, offset);
    }
}

fn fat_error_to_io_error(e: FatError) -> IoError {
    match e {
        FatError::NotFound => IoError::NotFound,
        FatError::FileHandleInvalid => IoError::FileHandleInvalid,
        FatError::OperationFailed => IoError::OperationFailed,
        FatError::UnsupportedOperation => IoError::UnsupportedOperation,
        FatError::AlreadyOpen => IoError::AlreadyOpen,
        FatError::InvalidArgument => IoError::InvalidArgument,
    }
}

/// Wrapper that implements AsyncDriver by delegating to FatDriver
struct IdosFatDriver {
    inner: FatDriver<IdosDiskIO>,
    log: SysLogger,
}

impl AsyncDriver for IdosFatDriver {
    fn release_buffer(&mut self, buffer_ptr: *mut u8, buffer_len: usize) {
        if idos_api::syscall::memory::unmap_memory(buffer_ptr as u32, buffer_len as u32).is_err() {
            self.log.log("failed to unmap shared buffer");
        }
    }

    fn open(&mut self, path: &str, flags: u32) -> Result<DriverFileReference, IoError> {
        self.log.log(path);
        self.inner
            .open(path, flags)
            .map(DriverFileReference::new)
            .map_err(fat_error_to_io_error)
    }

    fn read(
        &mut self,
        file_ref: DriverFileReference,
        buffer: &mut [u8],
        offset: u32,
    ) -> IoResult {
        self.log.log("read");
        self.inner
            .read(*file_ref, buffer, offset)
            .map_err(fat_error_to_io_error)
    }

    fn write(
        &mut self,
        file_ref: DriverFileReference,
        buffer: &[u8],
        offset: u32,
    ) -> IoResult {
        self.log.log("write");
        self.inner
            .write(*file_ref, buffer, offset)
            .map_err(fat_error_to_io_error)
    }

    fn close(&mut self, file_ref: DriverFileReference) -> IoResult {
        self.log.log("close");
        self.inner
            .close(*file_ref)
            .map_err(fat_error_to_io_error)
    }

    fn stat(
        &mut self,
        file_ref: DriverFileReference,
        status: &mut FileStatus,
    ) -> IoResult {
        let info = self.inner
            .stat(*file_ref)
            .map_err(fat_error_to_io_error)?;
        status.byte_size = info.byte_size;
        status.file_type = match info.file_type {
            FileTypeInfo::File => FileType::File as u32,
            FileTypeInfo::Dir => FileType::Dir as u32,
        };
        status.modification_time = info.modification_time;
        Ok(0)
    }

    fn mkdir(&mut self, path: &str) -> IoResult {
        self.inner.mkdir(path).map_err(fat_error_to_io_error)
    }

    fn unlink(&mut self, path: &str) -> IoResult {
        self.inner.unlink(path).map_err(fat_error_to_io_error)
    }

    fn rmdir(&mut self, path: &str) -> IoResult {
        self.inner.rmdir(path).map_err(fat_error_to_io_error)
    }

    fn rename(&mut self, old_path: &str, new_path: &str) -> IoResult {
        self.inner.rename(old_path, new_path).map_err(fat_error_to_io_error)
    }

    fn create_mapping(&mut self, path: &str) -> Result<DriverMappingToken, IoError> {
        self.inner
            .create_mapping(path)
            .map(DriverMappingToken::new)
            .map_err(fat_error_to_io_error)
    }

    fn remove_mapping(&mut self, map_token: DriverMappingToken) -> IoResult {
        self.inner
            .remove_mapping(*map_token)
            .map_err(fat_error_to_io_error)
    }

    fn page_in_mapping(
        &mut self,
        map_token: DriverMappingToken,
        offset_in_file: u32,
        frame_paddr: u32,
    ) -> IoResult {
        // In userspace, we need to map the physical frame into our address space,
        // fill it with data, then leave it mapped (address space leak, but functional).
        use idos_api::syscall::memory;

        let vaddr = match memory::map_memory(None, 0x1000, Some(frame_paddr)) {
            Ok(v) => v,
            Err(_) => return Err(IoError::OperationFailed),
        };

        let frame_buffer = unsafe { core::slice::from_raw_parts_mut(vaddr as *mut u8, 0x1000) };
        self.inner
            .page_in_mapping_to_buffer(*map_token, offset_in_file, frame_buffer)
            .map_err(fat_error_to_io_error)
    }
}

#[no_mangle]
pub extern "C" fn main() {
    // Handle 0 = args pipe reader (transferred by parent)
    // Handle 1 = response pipe writer (transferred by parent)
    let args_reader = Handle::new(0);
    let response_writer = Handle::new(1);

    // Read drive letter from pipe
    let mut len_buf: [u8; 1] = [0];
    let _ = read_sync(args_reader, &mut len_buf, 0);
    let drive_letter_len = len_buf[0] as usize;
    let mut drive_letter_buf: [u8; 8] = [0; 8];
    let _ = read_sync(args_reader, &mut drive_letter_buf[..drive_letter_len], 0);
    let drive_letter = unsafe { core::str::from_utf8_unchecked(&drive_letter_buf[..drive_letter_len]) };

    // Read device name from pipe
    let _ = read_sync(args_reader, &mut len_buf, 0);
    let dev_name_length = len_buf[0] as usize;
    let mut dev_name_buffer: [u8; 5 + 8] = [0; 5 + 8];
    dev_name_buffer[0..5].copy_from_slice("DEV:\\".as_bytes());
    let dev_name_len =
        5 + read_sync(args_reader, &mut dev_name_buffer[5..(5 + dev_name_length)], 0).unwrap() as usize;
    let _ = close_sync(args_reader);

    let dev_name = unsafe { core::str::from_utf8_unchecked(&dev_name_buffer[..dev_name_len]) };

    // Create the disk IO backed by the block device
    let disk_io = IdosDiskIO::new(dev_name);
    let fat_driver = FatDriver::new(disk_io, get_system_time);

    let mut driver_impl = IdosFatDriver {
        inner: fat_driver,
        log: SysLogger::new("FATFS"),
    };

    // Register ourselves as a filesystem driver
    register_fs(drive_letter);

    // Signal ready
    let _ = write_sync(response_writer, &[1], 0);
    let _ = close_sync(response_writer);

    // Enter message loop
    let messages = create_message_queue_handle();
    let mut incoming_message = Message::empty();

    loop {
        let msg_ptr = &mut incoming_message as *mut Message as u32;
        let msg_len = core::mem::size_of::<Message>() as u32;
        if let Ok(_sender) = io_sync(messages, idos_api::io::ASYNC_OP_READ, msg_ptr, msg_len, 0) {
            let request_id = incoming_message.unique_id;
            match driver_impl.handle_request(incoming_message) {
                Some(response) => driver_io_complete(request_id, response),
                None => (),
            }
        }
    }
}
