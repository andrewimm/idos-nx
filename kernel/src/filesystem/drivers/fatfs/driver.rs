use crate::collections::SlotList;
use crate::files::cursor::SeekMethod;
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
    fn open(&mut self, path: &str) -> u32 {
        crate::kprint!("FAT: Open \"{}\"\n", path);

        let root = self.fs.get_root_directory();
        let file = root.find_entry(path, &mut self.fs.disk).unwrap();
        let open_handle = OpenHandle {
            handle_object: HandleObject::File(file),
            cursor: 0,
        };
        let index = self.open_handle_map.insert(open_handle);
        index as u32
    }

    fn read(&mut self, instance: u32, buffer: &mut [u8]) -> u32 {
        let handle = self.open_handle_map.get(instance as usize).unwrap();
        let file = match handle.handle_object {
            HandleObject::File(f) => f.clone(),
            _ => panic!("Not a file, can't read"),
        };
        let cursor = handle.cursor;
        
        file.read(buffer, cursor, self.get_table(), self.get_disk_access())
    }

    fn write(&mut self, _instance: u32, _buffer: &[u8]) -> u32 {
        0
    }

    fn close(&mut self, handle: u32) {
        self.open_handle_map.remove(handle as usize);
    }

    fn seek(&mut self, instance: u32, offset: SeekMethod) -> u32 {
        let handle = self.open_handle_map.get_mut(instance as usize).unwrap();
        let new_cursor = match handle.handle_object {
            HandleObject::File(f) => {
                let mut new_cursor = offset.from_current_position(handle.cursor as usize) as u32;
                if new_cursor > f.byte_size() {
                    new_cursor = f.byte_size();
                }
                new_cursor
            },
            _ => panic!("Not a file, can't seek"),
        };
        handle.cursor = new_cursor;

        new_cursor
    }
}

