use alloc::{string::String, vec::Vec};

use super::{disk::DiskAccess, table::AllocationTable};

/// On-disk representation of a file or subdirectory
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct DirEntry {
    /// Short filename
    file_name: [u8; 8],
    /// File extension
    ext: [u8; 3],
    /// File attributes
    attributes: u8,
    /// Reserved byte used for various nonstandard things
    nonstandard_attributes: u8,
    /// Fine resolution of creation time, in 10ms units. Ranges from 0-199
    fine_create_time: u8,
    /// File creation time
    creation_time: FileTime,
    /// File creation date
    creation_date: FileDate,
    /// Last access date
    access_date: FileDate,
    /// Extended attributes
    extended_attributes: u16,
    /// Last modified time
    last_modify_time: FileTime,
    /// Last modified date
    last_modify_date: FileDate,
    /// First cluster of file data
    first_file_cluster: u16,
    /// File size in bytes
    byte_size: u32,
}

impl DirEntry {
    pub fn new() -> Self {
        Self {
            file_name: [0x20; 8],
            ext: [0x20; 3],
            attributes: 0,
            nonstandard_attributes: 0,
            fine_create_time: 0,
            creation_time: FileTime(0),
            creation_date: FileDate(0),
            access_date: FileDate(0),
            extended_attributes: 0,
            last_modify_time: FileTime(0),
            last_modify_date: FileDate(0),
            first_file_cluster: 0,
            byte_size: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self.file_name[0] {
            0x00 => true,
            0xe5 => true,
            _ => false,
        }
    }

    pub fn get_filename(&self) -> &str {
        let mut len = 8;
        for i in 0..8 {
            if self.file_name[i] == 0x20 {
                len = i;
                break;
            }
        }
        core::str::from_utf8(&self.file_name[..len]).unwrap_or("!!!!!!!!")
    }

    pub fn get_full_name(&self) -> String {
        let mut name = String::new();
        name.push_str(self.get_filename());
        name.push('.');
        name.push_str(self.get_ext());
        name
    }

    pub fn get_ext(&self) -> &str {
        core::str::from_utf8(&self.ext).unwrap_or("!!!")
    }

    pub fn is_directory(&self) -> bool {
        self.attributes & 0x10 != 0
    }
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct FileTime(u16);

impl FileTime {
    pub fn get_hours(&self) -> u16 {
        self.0 >> 11
    }

    pub fn get_minutes(&self) -> u16 {
        (self.0 >> 5) & 0x3f
    }

    pub fn get_seconds(&self) -> u16 {
        (self.0 & 0x1f) << 1
    }
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct FileDate(u16);

impl FileDate {
    pub fn get_year(&self) -> u32 {
        ((self.0 >> 9) & 0x7f) as u32 + 1980
    }

    pub fn get_month(&self) -> u16 {
        (self.0 >> 5) & 0xf
    }

    pub fn get_day(&self) -> u16 {
        self.0 & 0x1f
    }
}

pub struct RootDirectory {
    first_sector: u32,
    max_sectors: u32,
}

impl RootDirectory {
    pub fn new(first_sector: u32, max_entries: u32) -> Self {
        let max_sectors = max_entries * 32 / 512;
        Self {
            first_sector,
            max_sectors,
        }
    }

    pub fn iter<'disk>(&self, disk: &'disk mut DiskAccess) -> RootDirectoryIter<'disk> {
        let mut current = DirEntry::new();
        let dir_offset = self.first_sector * 512;
        disk.read_struct_from_disk(dir_offset, &mut current);

        RootDirectoryIter {
            disk,
            dir_offset,
            current_index: 0,
            current,
        }
    }

    pub fn find_entry(&self, name: &str, disk: &mut DiskAccess) -> Option<Entity> {
        let (filename, ext) = match name.rsplit_once('.') {
            Some(pair) => pair,
            None => (name, ""),
        };
        for entry in self.iter(disk) {
            if entry.get_filename() == filename && entry.get_ext() == ext {
                if entry.is_directory() {
                    return Some(Entity::Dir(Directory::from_dir_entry(entry)));
                } else {
                    return Some(Entity::File(File::from_dir_entry(entry)));
                }
            }
        }
        None
    }
}

pub struct RootDirectoryIter<'disk> {
    disk: &'disk mut DiskAccess,
    dir_offset: u32,
    current_index: u32,
    current: DirEntry,
}

impl Iterator for RootDirectoryIter<'_> {
    type Item = DirEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_empty() {
            return None;
        }

        let entry = self.current.clone();
        self.current_index += 1;
        let offset = self.dir_offset + self.current_index * core::mem::size_of::<DirEntry>() as u32;
        self.disk.read_struct_from_disk(offset, &mut self.current);

        Some(entry)
    }
}

pub enum Entity {
    Dir(Directory),
    File(File),
}

pub enum DirectoryType {
    Root(RootDirectory),
    Subdir(DirEntry),
}

pub struct Directory {
    dir_type: DirectoryType,
    entries_fetched: bool,
    entries: Vec<u8>,
}

impl Directory {
    pub fn from_dir_entry(dir_entry: DirEntry) -> Self {
        Self {
            dir_type: DirectoryType::Subdir(dir_entry),
            entries_fetched: false,
            entries: Vec::new(),
        }
    }

    pub fn from_root_dir(root: RootDirectory) -> Self {
        Self {
            dir_type: DirectoryType::Root(root),
            entries_fetched: false,
            entries: Vec::new(),
        }
    }

    pub fn read(&mut self, buffer: &mut [u8], offset: u32, table: AllocationTable, disk: &mut DiskAccess) -> u32 {
        if !self.entries_fetched {
            // the first time the directory IO handle is read, it caches the
            // entries it contains
            match &self.dir_type {
                DirectoryType::Root(root) => {
                    for entry in root.iter(disk) {
                        let name = entry.get_full_name();
                        self.entries.extend_from_slice(name.as_bytes());
                        self.entries.push(0);
                    }
                    self.entries_fetched = true;
                },
                DirectoryType::Subdir(entry) => {
                    // needs to be implemented
                    return 0;
                },
            }
        }

        let mut bytes_written = 0;
        let bytes_remaining = self.entries.len() - offset as usize;
        let bytes_to_write = bytes_remaining.min(buffer.len());
        while bytes_written < bytes_to_write {
            buffer[bytes_written] = *self.entries.get(offset as usize + bytes_written).unwrap();
            bytes_written += 1;
        }

        bytes_written as u32
    }
}

#[derive(Copy, Clone)]
pub struct File {
    dir_entry: DirEntry,
}

impl File {
    pub fn from_dir_entry(dir_entry: DirEntry) -> Self {
        Self {
            dir_entry,
        }
    }

    pub fn file_name(&self) -> String {
        let mut full_name = String::from(self.dir_entry.get_filename());
        full_name.push('.');
        full_name.push_str(self.dir_entry.get_ext());
        full_name
    }

    pub fn byte_size(&self) -> u32 {
        self.dir_entry.byte_size
    }

    pub fn read(&self, buffer: &mut [u8], offset: u32, table: AllocationTable, disk: &mut DiskAccess) -> u32 {
        let current_relative_cluster = offset / table.bytes_per_cluster();
        let cluster_offset = offset % table.bytes_per_cluster();
        let current_cluster = self.dir_entry.first_file_cluster as u32 + current_relative_cluster;
        let cluster_location = table.get_cluster_location(current_cluster);

        disk.read_bytes_from_disk(cluster_location + cluster_offset, buffer)
    }
}
