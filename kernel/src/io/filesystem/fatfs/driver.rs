use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};

use super::dir::{Directory, Entity, File};
use super::fs::FatFS;
use super::table::AllocationTable;
use crate::collections::SlotList;
use core::cell::RefCell;
use idos_api::io::driver::{AsyncDriver, DriverFileReference, DriverMappingToken};
use idos_api::io::error::IoError;
use idos_api::io::file::{FileStatus, FileType};

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

    fn create_mapping(&mut self, path: &str) -> Result<DriverMappingToken, IoError> {
        super::LOGGER.log(format_args!("CreateMapping \"{}\"", path));

        // Return existing token if this path is already mapped, incrementing refcount
        if let Some((token, refcount)) = self.mapping_tokens.get_mut(path) {
            *refcount += 1;
            return Ok(DriverMappingToken::new(*token));
        }

        let root = self.fs.borrow().get_root_directory();
        let entity = root
            .find_entry(path, &mut self.fs.borrow_mut().disk)
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
