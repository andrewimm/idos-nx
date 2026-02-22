use alloc::collections::BTreeMap;
use alloc::string::String;
use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::dir::{is_subdir_empty, parse_short_name, resolve_path, Directory, Entity, File, PathError};
use crate::disk::DiskIO;
use crate::fs::FatFS;
use crate::table::AllocationTable;

static ZERO_SECTOR: [u8; 512] = [0u8; 512];

/// A simple slot-based collection for handle management.
/// This replaces the kernel's SlotList without depending on kernel internals.
struct SlotList<T> {
    slots: alloc::vec::Vec<Option<T>>,
}

impl<T> SlotList<T> {
    fn new() -> Self {
        Self { slots: alloc::vec::Vec::new() }
    }

    fn insert(&mut self, item: T) -> usize {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(item);
                return i;
            }
        }
        self.slots.push(Some(item));
        self.slots.len() - 1
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.slots.get_mut(index)?.as_mut()
    }

    fn remove(&mut self, index: usize) -> Option<T> {
        if index < self.slots.len() {
            self.slots[index].take()
        } else {
            None
        }
    }
}

pub struct FatDriver<D: DiskIO> {
    fs: RefCell<FatFS<D>>,
    open_handle_map: SlotList<OpenHandle>,
    next_mapping_token: AtomicU32,
    mapping_tokens: BTreeMap<String, (u32, u32)>,
    mapping_files: BTreeMap<u32, File>,
    get_timestamp: fn() -> u32,
}

/// Error type that mirrors idos_api IoError but is platform-independent
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum FatError {
    NotFound = 2,
    FileHandleInvalid = 3,
    OperationFailed = 5,
    UnsupportedOperation = 6,
    AlreadyOpen = 8,
    InvalidArgument = 10,
}

pub type FatResult<T = u32> = Result<T, FatError>;

impl<D: DiskIO> FatDriver<D> {
    pub fn new(disk_io: D, get_timestamp: fn() -> u32) -> Self {
        Self {
            fs: RefCell::new(FatFS::new(disk_io)),
            open_handle_map: SlotList::new(),
            next_mapping_token: AtomicU32::new(1),
            mapping_tokens: BTreeMap::new(),
            mapping_files: BTreeMap::new(),
            get_timestamp,
        }
    }

    pub fn get_table(&self) -> AllocationTable {
        self.fs.borrow().table.clone()
    }

    pub fn open(&mut self, path: &str, flags: u32) -> FatResult<u32> {
        let create_flag = 0x1;
        let exclusive_flag = 0x2;

        let entity = if path.is_empty() {
            let root = self.fs.borrow().get_root_directory();
            Entity::Dir(Directory::from_root_dir(root))
        } else {
            let table = self.get_table();
            let root = self.fs.borrow().get_root_directory();
            let (parent_dir, leaf) = resolve_path(path, root, &table, &mut self.fs.borrow_mut().disk)
                .map_err(|e| match e {
                    PathError::NotFound => FatError::NotFound,
                    PathError::InvalidArgument => FatError::InvalidArgument,
                })?;

            let found = parent_dir.find_entry(leaf, &table, &mut self.fs.borrow_mut().disk);
            match found {
                Some(entity) => {
                    if flags & exclusive_flag != 0 && flags & create_flag != 0 {
                        return Err(FatError::AlreadyOpen);
                    }
                    entity
                }
                None => {
                    if flags & create_flag == 0 {
                        return Err(FatError::NotFound);
                    }
                    let (filename, ext) = parse_short_name(leaf);
                    let mut fs = self.fs.borrow_mut();
                    let disk_offset = parent_dir
                        .add_entry(&filename, &ext, 0x00, 0, &table, &mut fs.disk, self.get_timestamp)
                        .ok_or(FatError::OperationFailed)?;
                    let mut new_entry = crate::dir::DirEntry::new();
                    fs.disk.read_struct_from_disk(disk_offset, &mut new_entry);
                    fs.disk.flush_all();
                    Entity::File(File::from_dir_entry(new_entry, disk_offset))
                }
            }
        };
        let open_handle = OpenHandle {
            handle_entity: entity,
            cursor: 0,
        };
        let index = self.open_handle_map.insert(open_handle);
        Ok(index as u32)
    }

    pub fn read(
        &mut self,
        file_ref: u32,
        buffer: &mut [u8],
        offset: u32,
    ) -> FatResult {
        let table = self.get_table();
        let handle = self
            .open_handle_map
            .get_mut(file_ref as usize)
            .ok_or(FatError::FileHandleInvalid)?;
        let mut fs = self.fs.borrow_mut();
        let written = match &mut handle.handle_entity {
            Entity::File(f) => f.read(buffer, offset, table, &mut fs.disk),
            Entity::Dir(d) => d.read(buffer, offset, table, &mut fs.disk),
        };

        handle.cursor += written;

        Ok(written)
    }

    pub fn write(
        &mut self,
        file_ref: u32,
        buffer: &[u8],
        offset: u32,
    ) -> FatResult {
        let table = self.get_table();
        let handle = self
            .open_handle_map
            .get_mut(file_ref as usize)
            .ok_or(FatError::FileHandleInvalid)?;
        let mut fs = self.fs.borrow_mut();
        let written = match &mut handle.handle_entity {
            Entity::File(f) => f.write(buffer, offset, table, &mut fs.disk),
            Entity::Dir(_) => return Err(FatError::UnsupportedOperation),
        };
        handle.cursor += written;
        Ok(written)
    }

    pub fn close(&mut self, file_ref: u32) -> FatResult {
        if self.open_handle_map.remove(file_ref as usize).is_some() {
            self.fs.borrow_mut().disk.flush_all();
            Ok(0)
        } else {
            Err(FatError::FileHandleInvalid)
        }
    }

    pub fn stat(
        &mut self,
        file_ref: u32,
    ) -> FatResult<FileStatusInfo> {
        let handle = self
            .open_handle_map
            .get_mut(file_ref as usize)
            .ok_or(FatError::FileHandleInvalid)?;
        match &handle.handle_entity {
            Entity::File(f) => Ok(FileStatusInfo {
                byte_size: f.byte_size(),
                file_type: FileTypeInfo::File,
                modification_time: f.get_modification_time(),
            }),
            Entity::Dir(d) => Ok(FileStatusInfo {
                byte_size: 0,
                file_type: FileTypeInfo::Dir,
                modification_time: d.get_modification_time(),
            }),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> FatResult {
        let table = self.get_table();
        let root = self.fs.borrow().get_root_directory();
        let (parent_dir, leaf) = resolve_path(path, root, &table, &mut self.fs.borrow_mut().disk)
            .map_err(|e| match e {
                PathError::NotFound => FatError::NotFound,
                PathError::InvalidArgument => FatError::InvalidArgument,
            })?;

        let (filename, ext) = parse_short_name(leaf);

        if parent_dir.find_entry(leaf, &table, &mut self.fs.borrow_mut().disk).is_some() {
            return Err(FatError::AlreadyOpen);
        }

        let mut fs = self.fs.borrow_mut();

        let cluster = table.allocate_cluster(&mut fs.disk).ok_or(FatError::OperationFailed)?;

        let cluster_location = table.get_cluster_location(cluster);
        let bytes_per_cluster = table.bytes_per_cluster();
        let mut offset = 0u32;
        while offset < bytes_per_cluster {
            let to_write = (bytes_per_cluster - offset).min(512);
            fs.disk.write_bytes_to_disk(cluster_location + offset, &ZERO_SECTOR[..to_write as usize]);
            offset += to_write;
        }

        let parent_cluster = parent_dir.first_cluster() as u16;
        let new_subdir = crate::dir::SubDirectory::new(cluster);
        let dot_name: [u8; 8] = [b'.', 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20];
        let dotdot_name: [u8; 8] = [b'.', b'.', 0x20, 0x20, 0x20, 0x20, 0x20, 0x20];
        let no_ext: [u8; 3] = [0x20; 3];
        new_subdir.add_entry(&dot_name, &no_ext, 0x10, cluster as u16, &table, &mut fs.disk, self.get_timestamp);
        new_subdir.add_entry(&dotdot_name, &no_ext, 0x10, parent_cluster, &table, &mut fs.disk, self.get_timestamp);

        parent_dir
            .add_entry(&filename, &ext, 0x10, cluster as u16, &table, &mut fs.disk, self.get_timestamp)
            .ok_or(FatError::OperationFailed)?;

        fs.disk.flush_all();
        Ok(0)
    }

    pub fn unlink(&mut self, path: &str) -> FatResult {
        let table = self.get_table();
        let root = self.fs.borrow().get_root_directory();
        let (parent_dir, leaf) = resolve_path(path, root, &table, &mut self.fs.borrow_mut().disk)
            .map_err(|e| match e {
                PathError::NotFound => FatError::NotFound,
                PathError::InvalidArgument => FatError::InvalidArgument,
            })?;

        let (filename, ext) = parse_short_name(leaf);

        match parent_dir.find_entry(leaf, &table, &mut self.fs.borrow_mut().disk) {
            Some(Entity::File(_)) => {}
            Some(Entity::Dir(_)) => return Err(FatError::InvalidArgument),
            None => return Err(FatError::NotFound),
        }

        let mut fs = self.fs.borrow_mut();

        let removed = parent_dir.remove_entry(&filename, &ext, &table, &mut fs.disk)
            .ok_or(FatError::NotFound)?;

        let first_cluster = removed.first_file_cluster();
        if first_cluster != 0 {
            table.free_chain(first_cluster as u32, &mut fs.disk);
        }

        fs.disk.flush_all();
        Ok(0)
    }

    pub fn rmdir(&mut self, path: &str) -> FatResult {
        let table = self.get_table();
        let root = self.fs.borrow().get_root_directory();
        let (parent_dir, leaf) = resolve_path(path, root, &table, &mut self.fs.borrow_mut().disk)
            .map_err(|e| match e {
                PathError::NotFound => FatError::NotFound,
                PathError::InvalidArgument => FatError::InvalidArgument,
            })?;

        let (filename, ext) = parse_short_name(leaf);

        let first_cluster = match parent_dir.find_entry(leaf, &table, &mut self.fs.borrow_mut().disk) {
            Some(Entity::Dir(d)) => {
                match d.dir_type() {
                    crate::dir::DirectoryType::Subdir(entry) => entry.first_file_cluster(),
                    crate::dir::DirectoryType::Root(_) => return Err(FatError::InvalidArgument),
                }
            }
            Some(Entity::File(_)) => return Err(FatError::InvalidArgument),
            None => return Err(FatError::NotFound),
        };

        if !is_subdir_empty(first_cluster as u32, &table, &mut self.fs.borrow_mut().disk) {
            return Err(FatError::InvalidArgument);
        }

        let mut fs = self.fs.borrow_mut();

        parent_dir.remove_entry(&filename, &ext, &table, &mut fs.disk)
            .ok_or(FatError::NotFound)?;

        if first_cluster != 0 {
            table.free_chain(first_cluster as u32, &mut fs.disk);
        }

        fs.disk.flush_all();
        Ok(0)
    }

    pub fn rename(&mut self, old_path: &str, new_path: &str) -> FatResult {
        let table = self.get_table();

        let root = self.fs.borrow().get_root_directory();
        let (old_parent, old_leaf) = resolve_path(old_path, root, &table, &mut self.fs.borrow_mut().disk)
            .map_err(|e| match e {
                PathError::NotFound => FatError::NotFound,
                PathError::InvalidArgument => FatError::InvalidArgument,
            })?;

        if old_parent.find_entry(old_leaf, &table, &mut self.fs.borrow_mut().disk).is_none() {
            return Err(FatError::NotFound);
        }

        let root = self.fs.borrow().get_root_directory();
        let (new_parent, new_leaf) = resolve_path(new_path, root, &table, &mut self.fs.borrow_mut().disk)
            .map_err(|e| match e {
                PathError::NotFound => FatError::NotFound,
                PathError::InvalidArgument => FatError::InvalidArgument,
            })?;

        if new_parent.find_entry(new_leaf, &table, &mut self.fs.borrow_mut().disk).is_some() {
            return Err(FatError::AlreadyOpen);
        }

        let (old_filename, old_ext) = parse_short_name(old_leaf);
        let (new_filename, new_ext) = parse_short_name(new_leaf);

        let mut fs = self.fs.borrow_mut();

        let mut entry = old_parent.remove_entry(&old_filename, &old_ext, &table, &mut fs.disk)
            .ok_or(FatError::NotFound)?;

        entry.set_filename(&new_filename, &new_ext);

        new_parent.write_entry(&entry, &table, &mut fs.disk)
            .ok_or(FatError::OperationFailed)?;

        fs.disk.flush_all();
        Ok(0)
    }

    pub fn create_mapping(&mut self, path: &str) -> FatResult<u32> {
        if let Some((token, refcount)) = self.mapping_tokens.get_mut(path) {
            *refcount += 1;
            return Ok(*token);
        }

        let table = self.get_table();
        let root = self.fs.borrow().get_root_directory();
        let (parent_dir, leaf) = resolve_path(path, root, &table, &mut self.fs.borrow_mut().disk)
            .map_err(|e| match e {
                PathError::NotFound => FatError::NotFound,
                PathError::InvalidArgument => FatError::InvalidArgument,
            })?;

        let entity = parent_dir
            .find_entry(leaf, &table, &mut self.fs.borrow_mut().disk)
            .ok_or(FatError::NotFound)?;
        let file = match entity {
            Entity::File(f) => f,
            Entity::Dir(_) => return Err(FatError::InvalidArgument),
        };

        let token = self.next_mapping_token.fetch_add(1, Ordering::SeqCst);
        self.mapping_tokens.insert(String::from(path), (token, 1));
        self.mapping_files.insert(token, file);
        Ok(token)
    }

    pub fn remove_mapping(&mut self, map_token: u32) -> FatResult {
        let mut remove_path = None;
        for (path, (token, refcount)) in self.mapping_tokens.iter_mut() {
            if *token == map_token {
                *refcount -= 1;
                if *refcount == 0 {
                    remove_path = Some(path.clone());
                }
                break;
            }
        }
        match remove_path {
            Some(path) => {
                self.mapping_tokens.remove(&path);
                self.mapping_files.remove(&map_token);
            }
            None => {
                if !self.mapping_files.contains_key(&map_token) {
                    return Err(FatError::InvalidArgument);
                }
            }
        }
        Ok(1)
    }

    /// Read file data for a page-in mapping request.
    /// The caller provides a buffer to fill (typically 4096 bytes).
    /// Returns the number of bytes read.
    pub fn page_in_mapping_to_buffer(
        &mut self,
        map_token: u32,
        offset_in_file: u32,
        buffer: &mut [u8],
    ) -> FatResult {
        let table = self.get_table();
        let file = self
            .mapping_files
            .get_mut(&map_token)
            .ok_or(FatError::InvalidArgument)?;

        buffer.fill(0);

        let bytes_read = file.read(buffer, offset_in_file, table, &mut self.fs.borrow_mut().disk);
        Ok(bytes_read)
    }
}

pub struct OpenHandle {
    handle_entity: Entity,
    cursor: u32,
}

#[derive(Debug)]
pub enum FileTypeInfo {
    File,
    Dir,
}

pub struct FileStatusInfo {
    pub byte_size: u32,
    pub file_type: FileTypeInfo,
    pub modification_time: u32,
}
