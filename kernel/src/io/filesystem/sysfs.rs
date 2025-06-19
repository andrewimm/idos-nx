use crate::{
    collections::SlotList,
    files::{path::Path, stat::FileStatus},
    io::driver::kernel_driver::KernelDriver,
};
use idos_api::io::error::{IOError, IOResult};
use spin::RwLock;

use super::driver::AsyncIOCallback;

struct OpenFile {
    listing: ListingType,
}

enum ListingType {
    RootDir,
    Consoles,
    CPU,
    Drives,
    Memory,
}

impl ListingType {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "CONSOLES" => Some(Self::Consoles),
            "CPU" => Some(Self::CPU),
            "DRIVES" => Some(Self::Drives),
            "MEMORY" => Some(Self::Memory),
            _ => None,
        }
    }
}

const ROOT_LISTING: &str = "CONSOLES\0CPU\0DRIVES\0MEMORY\0";

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
            ListingType::RootDir => ROOT_LISTING,
            ListingType::Consoles => "Console0\0Console1\0",
            ListingType::CPU => "CPU0: Intel(R) Core(TM) i7-9700K CPU @ 3.60GHz\0",
            ListingType::Drives => "Drive0: 512GB SSD\0Drive1: 1TB HDD\0",
            ListingType::Memory => "Total: 16GB\0Used: 8GB\0Free: 8GB\0",
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
