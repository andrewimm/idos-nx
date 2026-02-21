use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};

use super::dir::{is_subdir_empty, parse_short_name, resolve_path, Directory, Entity, File};
use super::fs::FatFS;
use super::table::AllocationTable;
use crate::collections::SlotList;
use core::cell::RefCell;
use idos_api::io::driver::{AsyncDriver, DriverFileReference, DriverMappingToken};
use idos_api::io::error::IoError;
use idos_api::io::file::{FileStatus, FileType};

static ZERO_SECTOR: [u8; 512] = [0u8; 512];

pub struct FatDriver {
    fs: RefCell<FatFS>,
    open_handle_map: SlotList<OpenHandle>,
    next_mapping_token: AtomicU32,
    /// Maps path -> (token, refcount), so duplicate mappings to the same file reuse the token
    mapping_tokens: BTreeMap<String, (u32, u32)>,
    /// Maps token -> file, for page-in requests
    mapping_files: BTreeMap<u32, File>,
}

impl FatDriver {
    pub fn new(mount: &str) -> Self {
        Self {
            fs: RefCell::new(FatFS::new(mount)),
            open_handle_map: SlotList::new(),
            next_mapping_token: AtomicU32::new(1),
            mapping_tokens: BTreeMap::new(),
            mapping_files: BTreeMap::new(),
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

    fn open(&mut self, path: &str, flags: u32) -> Result<DriverFileReference, IoError> {
        use idos_api::io::{OPEN_FLAG_CREATE, OPEN_FLAG_EXCLUSIVE};

        super::LOGGER.log(format_args!("Open \"{}\" flags={}", path, flags));

        let entity = if path.is_empty() {
            let root = self.fs.borrow().get_root_directory();
            Entity::Dir(Directory::from_root_dir(root))
        } else {
            let table = self.get_table();
            let root = self.fs.borrow().get_root_directory();
            let (parent_dir, leaf) = resolve_path(path, root, &table, &mut self.fs.borrow_mut().disk)?;

            // Try to find existing entry first
            let found = parent_dir.find_entry(leaf, &table, &mut self.fs.borrow_mut().disk);
            match found {
                Some(entity) => {
                    if flags & OPEN_FLAG_EXCLUSIVE != 0 && flags & OPEN_FLAG_CREATE != 0 {
                        return Err(IoError::AlreadyOpen);
                    }
                    entity
                }
                None => {
                    if flags & OPEN_FLAG_CREATE == 0 {
                        return Err(IoError::NotFound);
                    }
                    // Create the file
                    let (filename, ext) = parse_short_name(leaf);
                    let mut fs = self.fs.borrow_mut();
                    let disk_offset = parent_dir
                        .add_entry(&filename, &ext, 0x00, 0, &table, &mut fs.disk)
                        .ok_or(IoError::OperationFailed)?;
                    // Read back the entry we just wrote
                    let mut new_entry = super::dir::DirEntry::new();
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

    fn write(
        &mut self,
        file_ref: DriverFileReference,
        buffer: &[u8],
        offset: u32,
    ) -> Result<u32, IoError> {
        let table = self.get_table();
        let handle = self
            .open_handle_map
            .get_mut(*file_ref as usize)
            .ok_or(IoError::FileHandleInvalid)?;
        let mut fs = self.fs.borrow_mut();
        let written = match &mut handle.handle_entity {
            Entity::File(f) => f.write(buffer, offset, table, &mut fs.disk),
            Entity::Dir(_) => return Err(IoError::UnsupportedOperation),
        };
        handle.cursor += written;
        Ok(written)
    }

    fn close(&mut self, file_ref: DriverFileReference) -> Result<u32, IoError> {
        if self.open_handle_map.remove(*file_ref as usize).is_some() {
            self.fs.borrow_mut().disk.flush_all();
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
            Entity::Dir(d) => {
                status.byte_size = 0;
                status.file_type = FileType::Dir as u32;
                status.modification_time = d.get_modification_time();
            }
        }
        Ok(0)
    }

    fn mkdir(&mut self, path: &str) -> Result<u32, IoError> {
        super::LOGGER.log(format_args!("Mkdir \"{}\"", path));

        let table = self.get_table();
        let root = self.fs.borrow().get_root_directory();
        let (parent_dir, leaf) = resolve_path(path, root, &table, &mut self.fs.borrow_mut().disk)?;

        let (filename, ext) = parse_short_name(leaf);

        // Check if entry already exists
        if parent_dir.find_entry(leaf, &table, &mut self.fs.borrow_mut().disk).is_some() {
            return Err(IoError::AlreadyOpen);
        }

        let mut fs = self.fs.borrow_mut();

        // Allocate a cluster for the new directory's contents
        let cluster = table.allocate_cluster(&mut fs.disk).ok_or(IoError::OperationFailed)?;

        // Zero-fill the new directory cluster
        let cluster_location = table.get_cluster_location(cluster);
        let bytes_per_cluster = table.bytes_per_cluster();
        let mut offset = 0u32;
        while offset < bytes_per_cluster {
            let to_write = (bytes_per_cluster - offset).min(512);
            fs.disk.write_bytes_to_disk(cluster_location + offset, &ZERO_SECTOR[..to_write as usize]);
            offset += to_write;
        }

        // Write "." and ".." entries into the new directory
        let parent_cluster = parent_dir.first_cluster() as u16;
        let new_subdir = super::dir::SubDirectory::new(cluster);
        let dot_name: [u8; 8] = [b'.', 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20];
        let dotdot_name: [u8; 8] = [b'.', b'.', 0x20, 0x20, 0x20, 0x20, 0x20, 0x20];
        let no_ext: [u8; 3] = [0x20; 3];
        new_subdir.add_entry(&dot_name, &no_ext, 0x10, cluster as u16, &table, &mut fs.disk);
        new_subdir.add_entry(&dotdot_name, &no_ext, 0x10, parent_cluster, &table, &mut fs.disk);

        // Add entry to parent directory (attribute 0x10 = directory)
        parent_dir
            .add_entry(&filename, &ext, 0x10, cluster as u16, &table, &mut fs.disk)
            .ok_or(IoError::OperationFailed)?;

        fs.disk.flush_all();
        Ok(0)
    }

    fn unlink(&mut self, path: &str) -> Result<u32, IoError> {
        super::LOGGER.log(format_args!("Unlink \"{}\"", path));

        let table = self.get_table();
        let root = self.fs.borrow().get_root_directory();
        let (parent_dir, leaf) = resolve_path(path, root, &table, &mut self.fs.borrow_mut().disk)?;

        let (filename, ext) = parse_short_name(leaf);

        // Check that the entry exists and is a file
        match parent_dir.find_entry(leaf, &table, &mut self.fs.borrow_mut().disk) {
            Some(Entity::File(_)) => {}
            Some(Entity::Dir(_)) => return Err(IoError::InvalidArgument),
            None => return Err(IoError::NotFound),
        }

        let mut fs = self.fs.borrow_mut();

        // Remove the directory entry
        let removed = parent_dir.remove_entry(&filename, &ext, &table, &mut fs.disk)
            .ok_or(IoError::NotFound)?;

        // Free the cluster chain
        let first_cluster = removed.first_file_cluster();
        if first_cluster != 0 {
            table.free_chain(first_cluster as u32, &mut fs.disk);
        }

        fs.disk.flush_all();
        Ok(0)
    }

    fn rmdir(&mut self, path: &str) -> Result<u32, IoError> {
        super::LOGGER.log(format_args!("Rmdir \"{}\"", path));

        let table = self.get_table();
        let root = self.fs.borrow().get_root_directory();
        let (parent_dir, leaf) = resolve_path(path, root, &table, &mut self.fs.borrow_mut().disk)?;

        let (filename, ext) = parse_short_name(leaf);

        // Check that the entry exists and is a directory
        let first_cluster = match parent_dir.find_entry(leaf, &table, &mut self.fs.borrow_mut().disk) {
            Some(Entity::Dir(d)) => {
                match d.dir_type() {
                    super::dir::DirectoryType::Subdir(entry) => entry.first_file_cluster(),
                    super::dir::DirectoryType::Root(_) => return Err(IoError::InvalidArgument),
                }
            }
            Some(Entity::File(_)) => return Err(IoError::InvalidArgument),
            None => return Err(IoError::NotFound),
        };

        // Check that directory is empty
        if !is_subdir_empty(first_cluster as u32, &table, &mut self.fs.borrow_mut().disk) {
            return Err(IoError::InvalidArgument);
        }

        let mut fs = self.fs.borrow_mut();

        // Remove the directory entry
        parent_dir.remove_entry(&filename, &ext, &table, &mut fs.disk)
            .ok_or(IoError::NotFound)?;

        // Free the cluster chain
        if first_cluster != 0 {
            table.free_chain(first_cluster as u32, &mut fs.disk);
        }

        fs.disk.flush_all();
        Ok(0)
    }

    fn rename(&mut self, old_path: &str, new_path: &str) -> Result<u32, IoError> {
        super::LOGGER.log(format_args!("Rename \"{}\" -> \"{}\"", old_path, new_path));

        let table = self.get_table();

        // Resolve old path
        let root = self.fs.borrow().get_root_directory();
        let (old_parent, old_leaf) = resolve_path(old_path, root, &table, &mut self.fs.borrow_mut().disk)?;

        // Check that the source entry exists
        if old_parent.find_entry(old_leaf, &table, &mut self.fs.borrow_mut().disk).is_none() {
            return Err(IoError::NotFound);
        }

        // Resolve new path
        let root = self.fs.borrow().get_root_directory();
        let (new_parent, new_leaf) = resolve_path(new_path, root, &table, &mut self.fs.borrow_mut().disk)?;

        // Check that the destination does not already exist
        if new_parent.find_entry(new_leaf, &table, &mut self.fs.borrow_mut().disk).is_some() {
            return Err(IoError::AlreadyOpen);
        }

        let (old_filename, old_ext) = parse_short_name(old_leaf);
        let (new_filename, new_ext) = parse_short_name(new_leaf);

        let mut fs = self.fs.borrow_mut();

        // Remove the old entry (marks it as deleted, returns the DirEntry data)
        let mut entry = old_parent.remove_entry(&old_filename, &old_ext, &table, &mut fs.disk)
            .ok_or(IoError::NotFound)?;

        // Update the filename to the new name
        entry.set_filename(&new_filename, &new_ext);

        // Write the entry into the new parent directory
        new_parent.write_entry(&entry, &table, &mut fs.disk)
            .ok_or(IoError::OperationFailed)?;

        fs.disk.flush_all();
        Ok(0)
    }

    fn create_mapping(&mut self, path: &str) -> Result<DriverMappingToken, IoError> {
        super::LOGGER.log(format_args!("CreateMapping \"{}\"", path));

        // Return existing token if this path is already mapped, incrementing refcount
        if let Some((token, refcount)) = self.mapping_tokens.get_mut(path) {
            *refcount += 1;
            return Ok(DriverMappingToken::new(*token));
        }

        let table = self.get_table();
        let root = self.fs.borrow().get_root_directory();
        let (parent_dir, leaf) = resolve_path(path, root, &table, &mut self.fs.borrow_mut().disk)?;

        let entity = parent_dir
            .find_entry(leaf, &table, &mut self.fs.borrow_mut().disk)
            .ok_or(IoError::NotFound)?;
        let file = match entity {
            Entity::File(f) => f,
            Entity::Dir(_) => return Err(IoError::InvalidArgument),
        };

        let token = self.next_mapping_token.fetch_add(1, Ordering::SeqCst);
        self.mapping_tokens.insert(String::from(path), (token, 1));
        self.mapping_files.insert(token, file);
        Ok(DriverMappingToken::new(token))
    }

    fn remove_mapping(&mut self, map_token: DriverMappingToken) -> Result<u32, IoError> {
        // Decrement refcount; only remove when it hits zero
        let mut remove_path = None;
        for (path, (token, refcount)) in self.mapping_tokens.iter_mut() {
            if *token == *map_token {
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
                self.mapping_files.remove(&*map_token);
            }
            None => {
                // Token not found at all — check if it exists in mapping_files
                if !self.mapping_files.contains_key(&*map_token) {
                    return Err(IoError::InvalidArgument);
                }
                // Refcount decremented but not zero, nothing else to do
            }
        }
        Ok(1)
    }

    fn page_in_mapping(
        &mut self,
        map_token: DriverMappingToken,
        offset_in_file: u32,
        frame_paddr: u32,
    ) -> Result<u32, IoError> {
        use crate::memory::address::PhysicalAddress;
        use crate::memory::virt::scratch::UnmappedPage;

        let table = self.get_table();
        let file = self
            .mapping_files
            .get_mut(&*map_token)
            .ok_or(IoError::InvalidArgument)?;

        let frame_page = UnmappedPage::map(PhysicalAddress::new(frame_paddr));
        let frame_buffer_ptr = frame_page.virtual_address().as_ptr_mut::<u8>();
        let frame_buffer = unsafe { core::slice::from_raw_parts_mut(frame_buffer_ptr, 0x1000) };

        // Zero-fill first, then read file data over it
        frame_buffer.fill(0);

        let bytes_read = file.read(frame_buffer, offset_in_file, table, &mut self.fs.borrow_mut().disk);
        Ok(bytes_read)
    }
}
