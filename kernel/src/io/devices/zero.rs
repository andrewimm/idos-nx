use crate::io::driver::kernel_driver::KernelDriver;
use crate::io::filesystem::driver::AsyncIOCallback;
use crate::memory::address::PhysicalAddress;
use crate::{files::path::Path, memory::virt::scratch::UnmappedPage};
use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU32, Ordering};
use idos_api::io::{
    driver::DriverMappingToken,
    error::{IoError, IoResult},
};
use spin::RwLock;

pub struct ZeroDev {
    next_instance: AtomicU32,
    open_files: RwLock<BTreeMap<u32, OpenFile>>,
}

struct OpenFile {}

impl OpenFile {
    pub fn new() -> Self {
        Self {}
    }
}

impl ZeroDev {
    pub fn new() -> Self {
        Self {
            next_instance: AtomicU32::new(1),
            open_files: RwLock::new(BTreeMap::new()),
        }
    }

    fn open_impl(&self) -> IoResult {
        let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
        self.open_files.write().insert(instance, OpenFile::new());
        Ok(instance)
    }

    fn read_impl(&self, instance: u32, buffer: &mut [u8]) -> IoResult {
        let mut open_files = self.open_files.write();
        let _found = open_files
            .get_mut(&instance)
            .ok_or(IoError::FileHandleInvalid)?;
        for i in 0..buffer.len() {
            buffer[i] = 0;
        }
        Ok(buffer.len() as u32)
    }

    fn close_impl(&self, instance: u32) -> IoResult {
        let mut open_files = self.open_files.write();
        open_files
            .remove(&instance)
            .ok_or(IoError::FileHandleInvalid)?;
        Ok(0)
    }
}

impl KernelDriver for ZeroDev {
    fn open(&self, _path: Option<Path>, _flags: u32, _: AsyncIOCallback) -> Option<IoResult> {
        Some(self.open_impl())
    }

    fn read(
        &self,
        instance: u32,
        buffer: &mut [u8],
        _: u32,
        _: AsyncIOCallback,
    ) -> Option<IoResult> {
        Some(self.read_impl(instance, buffer))
    }

    fn close(&self, instance: u32, _: AsyncIOCallback) -> Option<IoResult> {
        Some(self.close_impl(instance))
    }

    fn create_mapping(&self, path: &str) -> Option<IoResult<DriverMappingToken>> {
        Some(Ok(DriverMappingToken::new(1)))
    }

    fn remove_mapping(&self, map_token: DriverMappingToken) -> Option<IoResult> {
        Some(Ok(1))
    }

    fn page_in_mapping(
        &self,
        map_token: DriverMappingToken,
        offset_in_file: u32,
        frame_paddr: u32,
    ) -> Option<IoResult> {
        let mapped_page = UnmappedPage::map(PhysicalAddress::new(frame_paddr));
        let buffer_ptr = mapped_page.virtual_address().as_ptr_mut::<u8>();
        let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr, 0x1000) };
        buffer.fill(0);
        Some(Ok(0x1000))
    }
}
