use crate::collections::SlotList;
use crate::files::cursor::SeekMethod;
use crate::files::error::IOError;
use crate::files::stat::FileStatus;
use crate::filesystem::drivers::asyncfs::AsyncDriver;
use super::disk::DiskAccess;
use super::fs::FatFS;
use super::dir::{Directory, File};
use super::table::AllocationTable;

pub struct FatDriver {
    fs: FatFS,
    open_handle_map: SlotList<OpenHandle>,
}

impl FatDriver {
    pub fn new(mount: &str) -> Self {
        Self {
            fs: FatFS::new(mount),
            open_handle_map: SlotList::new(),
        }
    }

    pub fn get_disk_access(&mut self) -> &mut DiskAccess {
        &mut self.fs.disk
    }

    pub fn get_table(&self) -> AllocationTable {
        self.fs.table.clone()
    }
}

pub struct OpenHandle {
    handle_object: HandleObject,
    cursor: u32,
}

#[derive(Copy, Clone)]
pub enum HandleObject {
    Dir(Directory),
    File(File),
}

impl AsyncDriver for FatDriver {
    fn open(&mut self, path: &str) -> Result<u32, IOError> {
        crate::kprint!("FAT: Open \"{}\"\n", path);

        let root = self.fs.get_root_directory();
        let file = root.find_entry(path, &mut self.fs.disk).ok_or(IOError::NotFound)?;
        let open_handle = OpenHandle {
            handle_object: HandleObject::File(file),
            cursor: 0,
        };
        let index = self.open_handle_map.insert(open_handle);
        Ok(index as u32)
    }

    fn read(&mut self, instance: u32, buffer: &mut [u8]) -> Result<u32, IOError> {
        let handle = self.open_handle_map.get(instance as usize).ok_or(IOError::FileHandleInvalid)?;
        let file = match handle.handle_object {
            HandleObject::File(f) => f.clone(),
            _ => return Err(IOError::FileHandleWrongType),
        };
        let cursor = handle.cursor;
        
        // TODO: Integrate more error handling into the actual file reading
        Ok(file.read(buffer, cursor, self.get_table(), self.get_disk_access()))
    }

    fn write(&mut self, _instance: u32, _buffer: &[u8]) -> Result<u32, IOError> {
        Err(IOError::OperationFailed)
    }

    fn close(&mut self, handle: u32) -> Result<(), IOError> {
        if self.open_handle_map.remove(handle as usize).is_some() {
            Ok(())
        } else {
            Err(IOError::FileHandleInvalid)
        }
    }

    fn seek(&mut self, instance: u32, offset: SeekMethod) -> Result<u32, IOError> {
        let handle = self.open_handle_map.get_mut(instance as usize).ok_or(IOError::FileHandleInvalid)?;
        let new_cursor = match handle.handle_object {
            HandleObject::File(f) => {
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

    fn stat(&mut self, instance: u32, status: &mut FileStatus) -> Result<(), IOError> {
        let handle = self.open_handle_map.get_mut(instance as usize).ok_or(IOError::FileHandleInvalid)?;
        match handle.handle_object {
            HandleObject::File(f) => {
                status.byte_size = f.byte_size();
                status.file_type = 1;
            },
            _ => panic!("Need to implement stat for other handle types"),
        }
        Ok(())
    }
}

