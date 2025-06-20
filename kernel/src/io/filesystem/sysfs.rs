use crate::{
    collections::SlotList,
    files::{path::Path, stat::FileStatus},
    io::driver::kernel_driver::KernelDriver,
    memory::physical::with_allocator,
};
use alloc::string::String;
use idos_api::io::error::{IOError, IOResult};
use spin::RwLock;

use super::{driver::AsyncIOCallback, get_all_drive_names};

struct OpenFile {
    listing: ListingType,
}

enum ListingType {
    RootDir,
    CPU,
    Drives,
    Memory,
}

impl ListingType {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "CPU" => Some(Self::CPU),
            "DRIVES" => Some(Self::Drives),
            "MEMORY" => Some(Self::Memory),
            _ => None,
        }
    }
}

const ROOT_LISTING: &str = "CPU\0DRIVES\0MEMORY\0";

pub struct SysFS {
    open_files: RwLock<SlotList<OpenFile>>,
}

impl SysFS {
    pub fn new() -> Self {
        Self {
            open_files: RwLock::new(SlotList::new()),
        }
    }

    fn open_impl(&self, path: Path) -> IOResult {
        let open_file = if path.is_empty() {
            OpenFile {
                listing: ListingType::RootDir,
            }
        } else if let Some(listing_type) = ListingType::from_str(path.as_str()) {
            OpenFile {
                listing: listing_type,
            }
        } else {
            return Err(IOError::NotFound);
        };
        let index = self.open_files.write().insert(open_file);
        Ok(index as u32)
    }

    fn read_impl(&self, instance: u32, buffer: &mut [u8], offset: u32) -> IOResult {
        let mut open_files = self.open_files.write();
        let open_file = open_files
            .get_mut(instance as usize)
            .ok_or(IOError::FileHandleInvalid)?;
        let content_string = match open_file.listing {
            ListingType::RootDir => String::from(ROOT_LISTING),
            ListingType::CPU => Self::generate_cpu_content(),
            ListingType::Drives => Self::generate_drives_content(),
            ListingType::Memory => Self::generate_memory_content(),
        };
        let content_bytes = content_string.as_bytes();
        if offset >= content_bytes.len() as u32 {
            return Ok(0);
        }
        let offset_slice = &content_bytes[(offset as usize)..];
        let to_write = offset_slice.len().min(buffer.len());
        buffer[..to_write].copy_from_slice(&offset_slice[..to_write]);
        Ok(to_write as u32)
    }

    fn generate_cpu_content() -> String {
        let mut content = String::new();

        let user_time = 0;
        let kernel_time = 0;
        let idle_time = 0;
        content.push_str(&alloc::format!(
            "CPU Usage:\nUser Time: {}\nKernel Time: {}\nIdle Time: {}",
            user_time,
            kernel_time,
            idle_time
        ));

        content
    }

    fn generate_drives_content() -> String {
        let mut names = get_all_drive_names();
        names.push(String::from("DEV"));
        names.sort();

        names.join("\n")
    }

    fn generate_memory_content() -> String {
        let (total, free) = with_allocator(|a| (a.total_frame_count(), a.get_free_frame_count()));
        let total_memory = total * 4; // in KiB
        let free_memory = free * 4; // in KiB

        alloc::format!(
            "Total Memory: {} KiB\nFree Memory: {} KiB",
            total_memory,
            free_memory,
        )
    }

    fn stat_impl(&self, instance: u32, file_status: &mut FileStatus) -> IOResult {
        let open_files = self.open_files.read();
        let open_file = open_files
            .get(instance as usize)
            .ok_or(IOError::FileHandleInvalid)?;
        file_status.byte_size = 0;
        file_status.file_type = 1;
        file_status.modification_time = 0;
        Ok(1)
    }
}

impl KernelDriver for SysFS {
    fn open(&self, path: Option<Path>, _io_callback: AsyncIOCallback) -> Option<IOResult> {
        match path {
            Some(p) => Some(self.open_impl(p)),
            None => Some(Err(IOError::NotFound)),
        }
    }

    fn read(
        &self,
        instance: u32,
        buffer: &mut [u8],
        offset: u32,
        _io_callback: AsyncIOCallback,
    ) -> Option<IOResult> {
        Some(self.read_impl(instance, buffer, offset))
    }

    fn stat(
        &self,
        instance: u32,
        file_status: &mut FileStatus,
        _io_callback: AsyncIOCallback,
    ) -> Option<IOResult> {
        Some(self.stat_impl(instance, file_status))
    }

    fn close(&self, instance: u32, io_callback: AsyncIOCallback) -> Option<IOResult> {
        if self.open_files.write().remove(instance as usize).is_none() {
            Some(Err(IOError::FileHandleInvalid))
        } else {
            Some(Ok(1))
        }
    }
}
