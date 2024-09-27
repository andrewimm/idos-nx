use alloc::string::String;
use spin::RwLock;
use crate::collections::SlotList;
use crate::files::path::Path;
use crate::io::IOError;
use crate::io::driver::comms::IOResult;
use crate::io::driver::kernel_driver::KernelDriver;

use super::driver::AsyncIOCallback;

/// Device drivers are listed and accessible under the DEV: virtual drive.
/// To avoid unnecessary abstraction, though, FS drivers and device drivers are
/// stored in a single unified location. Accessing a device on the DEV: drive
/// does not require opening a filesystem driver first; it will recognize the
/// DEV: name and use the rest of the file path to look up a matching device
/// driver.
/// This is fine until a program (such as the DIR command) tries to read the
/// root directory listing of the DEV: drive. This is supposed to list all
/// device drivers by name. To enable this, a fake FS is used to only store
/// root listing calls. They will be immediately resolved, but progress needs
/// to be stored in case it takes multiple reads to complete the listing.
pub struct DevFileSystem {
    root_listings: RwLock<SlotList<RootListing>>,
}

impl DevFileSystem {
    pub const fn new() -> Self {
        Self {
            root_listings: RwLock::new(SlotList::new()),
        }
    }

    pub fn open_root_listing(&self) -> u32 {
        let names = super::get_all_dev_names();
        let mut content = String::new();
        for name in names {
            content.push_str(name.as_str());
            content.push('\0');
        }
        let index = self.root_listings
            .write()
            .insert(
                RootListing {
                    cursor: 0,
                    content,
                }
            );
        index as u32
    }

    pub fn close_root_listing(&self, instance: u32) -> IOResult {
        self.root_listings.write().remove(instance as usize).ok_or(IOError::FileHandleInvalid).map(|_| 1)
    }

    pub fn read_listing(&self, instance: u32, buffer: &mut [u8], offset: usize) -> IOResult {
        let mut listings = self.root_listings.write();
        let listing = listings.get_mut(instance as usize).ok_or(IOError::FileHandleInvalid)?;
        let content_bytes = listing.content.as_bytes();
        let capped_offset = offset.min(content_bytes.len());
        let bytes_unread = content_bytes.len() - capped_offset;
        let to_write = bytes_unread.min(buffer.len());
        let copy_start = capped_offset;
        let copy_end = copy_start + to_write;
        buffer[..to_write].copy_from_slice(&content_bytes[copy_start..copy_end]);
        Ok(to_write as u32)

    }
}

impl KernelDriver for DevFileSystem {
    fn open(&self, _path: Option<Path>, _io_callback: AsyncIOCallback) -> Option<IOResult> {
        Some(Ok(self.open_root_listing()))
    }

    fn read(&self, instance: u32, buffer: &mut [u8], offset: u32, _io_callback: AsyncIOCallback) -> Option<IOResult> {
        Some(self.read_listing(instance, buffer, offset as usize))
    }

    fn close(&self, instance: u32, _io_callback: AsyncIOCallback) -> Option<IOResult> {
        Some(self.close_root_listing(instance))
    }
}

struct RootListing {
    cursor: usize,
    content: String,
}

