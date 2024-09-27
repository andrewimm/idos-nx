use core::cell::RefCell;

use crate::collections::SlotList;
use crate::files::cursor::SeekMethod;
use crate::io::IOError;
use crate::files::stat::FileStatus;
use crate::io::driver::async_driver::AsyncDriver;
use super::fs::FatFS;
use super::dir::{Directory, Entity, File};
use super::table::AllocationTable;

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
    fn open(&mut self, path: &str) -> Result<u32, IOError> {
        crate::kprint!("FAT: Open \"{}\"\n", path);

        let root = self.fs.borrow().get_root_directory();
        let entity = if path.is_empty() {
            Entity::Dir(Directory::from_root_dir(root))
        } else {
            root.find_entry(path, &mut self.fs.borrow_mut().disk).ok_or(IOError::NotFound)?
        };
        let open_handle = OpenHandle {
            handle_entity: entity,
            cursor: 0,
        };
        let index = self.open_handle_map.insert(open_handle);
        Ok(index as u32)
    }

    fn read(&mut self, instance: u32, buffer: &mut [u8], offset: u32) -> Result<u32, IOError> {
        crate::kprintln!("FAT: Read");
        let table = self.get_table();
        let handle = self.open_handle_map.get_mut(instance as usize).ok_or(IOError::FileHandleInvalid)?;
        let mut fs = self.fs.borrow_mut();
        let written = match &mut handle.handle_entity {
            Entity::File(f) => {
                f.read(buffer, offset, table, &mut fs.disk)
            },
            Entity::Dir(d) => {
                d.read(buffer, offset, table, &mut fs.disk)
            },
        };

        handle.cursor += written;
        
        Ok(written)
    }

    fn close(&mut self, handle: u32) -> Result<u32, IOError> {
        if self.open_handle_map.remove(handle as usize).is_some() {
            Ok(0)
        } else {
            Err(IOError::FileHandleInvalid)
        }
    }

    /*

    fn seek(&mut self, instance: u32, offset: SeekMethod) -> Result<u32, IOError> {
        let handle = self.open_handle_map.get_mut(instance as usize).ok_or(IOError::FileHandleInvalid)?;
        let new_cursor = match handle.handle_entity {
            Entity::File(f) => {
                let mut new_cursor = offset.from_current_position(handle.cursor as usize) as u32;
                if new_cursor > f.byte_size() {
                    new_cursor = f.byte_size();
                }
                new_cursor
            },
            _ => return Err(IOError::FileHandleWrongType),
        };
        handle.cursor = new_cursor;

        Ok(new_cursor)
    }

    */

    fn stat(&mut self, instance: u32, status: &mut FileStatus) -> Result<u32, IOError> {
        let handle = self.open_handle_map.get_mut(instance as usize).ok_or(IOError::FileHandleInvalid)?;
        match handle.handle_entity {
            Entity::File(f) => {
                status.byte_size = f.byte_size();
                status.file_type = 1;
                status.modification_time = f.get_modification_time();
            },
            _ => panic!("Need to implement stat for other handle types"),
        }
        Ok(0)
    }
}

