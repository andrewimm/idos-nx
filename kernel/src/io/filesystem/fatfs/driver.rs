use super::dir::{Directory, Entity};
use super::fs::FatFS;
use super::table::AllocationTable;
use crate::collections::SlotList;
use core::cell::RefCell;
use idos_api::io::driver::{AsyncDriver, DriverFileReference};
use idos_api::io::error::IoError;
use idos_api::io::file::{FileStatus, FileType};

pub struct FatDriver {
    fs: RefCell<FatFS>,
    open_handle_map: SlotList<OpenHandle>,
}

impl FatDriver {
    pub fn new(mount: &str) -> Self {
        Self {
            fs: RefCell::new(FatFS::new(mount)),
            open_handle_map: SlotList::new(),
        }
    }

    pub fn get_table(&self) -> AllocationTable {
        self.fs.borrow().table.clone()
    }
}

pub struct OpenHandle {
    handle_entity: Entity,
    cursor: u32,
}

impl AsyncDriver for FatDriver {
    fn release_buffer(&mut self, buffer_ptr: *mut u8, buffer_len: usize) {
        use crate::memory::{address::VirtualAddress, shared::release_buffer};
        release_buffer(VirtualAddress::new(buffer_ptr as u32), buffer_len);
    }

    fn open(&mut self, path: &str) -> Result<DriverFileReference, IoError> {
        super::LOGGER.log(format_args!("Open \"{}\"", path));

        let root = self.fs.borrow().get_root_directory();
        let entity = if path.is_empty() {
            Entity::Dir(Directory::from_root_dir(root))
        } else {
            root.find_entry(path, &mut self.fs.borrow_mut().disk)
                .ok_or(IoError::NotFound)?
        };
        let open_handle = OpenHandle {
            handle_entity: entity,
            cursor: 0,
        };
        let index = self.open_handle_map.insert(open_handle);
        Ok(DriverFileReference::new(index as u32))
    }

    fn read(
        &mut self,
        file_ref: DriverFileReference,
        buffer: &mut [u8],
        offset: u32,
    ) -> Result<u32, IoError> {
        let table = self.get_table();
        let handle = self
            .open_handle_map
            .get_mut(*file_ref as usize)
            .ok_or(IoError::FileHandleInvalid)?;
        let mut fs = self.fs.borrow_mut();
        let written = match &mut handle.handle_entity {
            Entity::File(f) => f.read(buffer, offset, table, &mut fs.disk),
            Entity::Dir(d) => d.read(buffer, offset, table, &mut fs.disk),
        };

        handle.cursor += written;

        Ok(written)
    }

    fn close(&mut self, file_ref: DriverFileReference) -> Result<u32, IoError> {
        if self.open_handle_map.remove(*file_ref as usize).is_some() {
            Ok(0)
        } else {
            Err(IoError::FileHandleInvalid)
        }
    }

    fn stat(
        &mut self,
        file_ref: DriverFileReference,
        status: &mut FileStatus,
    ) -> Result<u32, IoError> {
        let handle = self
            .open_handle_map
            .get_mut(*file_ref as usize)
            .ok_or(IoError::FileHandleInvalid)?;
        match &handle.handle_entity {
            Entity::File(f) => {
                status.byte_size = f.byte_size();
                status.file_type = FileType::File as u32;
                status.modification_time = f.get_modification_time();
            }
            Entity::Dir(_) => {
                status.byte_size = 0;
                status.file_type = FileType::Dir as u32;
                status.modification_time = 0;
            }
        }
        Ok(0)
    }
}
