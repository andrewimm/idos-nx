use alloc::{string::String, vec::Vec};

use crate::time::{
    date::{Date, DateTime, Time},
    system::Timestamp,
};

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
        if self.ext[0] != 0x20 {
            name.push('.');
            name.push_str(self.get_ext());
        }
        name
    }

    pub fn get_ext(&self) -> &str {
        let mut len = 3;
        for i in 0..3 {
            if self.ext[i] == 0x20 {
                len = i;
                break;
            }
        }
        core::str::from_utf8(&self.ext[..len]).unwrap_or("!!!")
    }

    pub fn get_modification_timestamp(&self) -> Timestamp {
        let mod_time = self.last_modify_time;
        let mod_date = self.last_modify_date;
        DateTime {
            date: mod_date.to_system_date(),
            time: mod_time.to_system_time(),
        }
        .to_timestamp()
    }

    pub fn is_directory(&self) -> bool {
        self.attributes & 0x10 != 0
    }

    pub fn matches_name(&self, filename: &[u8; 8], ext: &[u8; 3]) -> bool {
        // TODO: make case sensitivity configurable
        for i in 0..8 {
            if ascii_char_matches(self.file_name[i], filename[i]) {
                continue;
            }
            return false;
        }
        for i in 0..3 {
            if ascii_char_matches(self.ext[i], ext[i]) {
                continue;
            }
            return false;
        }
        true
    }
}

fn ascii_char_matches(a: u8, b: u8) -> bool {
    if a > 0x40 && a < 0x5b {
        return a == b || (a + 0x20) == b;
    }
    if a > 0x60 && a < 0x7b {
        return a == b || a == (b + 0x20);
    }
    return a == b;
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

    pub fn to_system_time(&self) -> Time {
        Time {
            hours: self.get_hours() as u8,
            minutes: self.get_minutes() as u8,
            seconds: self.get_seconds() as u8,
        }
    }
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct FileDate(u16);

impl FileDate {
    pub fn get_year(&self) -> u16 {
        ((self.0 >> 9) & 0x7f) + 1980
    }

    pub fn get_month(&self) -> u16 {
        (self.0 >> 5) & 0xf
    }

    pub fn get_day(&self) -> u16 {
        self.0 & 0x1f
    }

    pub fn to_system_date(&self) -> Date {
        Date {
            day: self.get_day() as u8,
            month: self.get_month() as u8,
            year: self.get_year(),
        }
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
        let mut short_filename: [u8; 8] = [0x20; 8];
        let mut short_ext: [u8; 3] = [0x20; 3];
        let filename_len = filename.len().min(8);
        let ext_len = ext.len().min(3);
        short_filename[..filename_len].copy_from_slice(&filename.as_bytes()[..filename_len]);
        short_ext[..ext_len].copy_from_slice(&ext.as_bytes()[..ext_len]);

        for entry in self.iter(disk) {
            if entry.matches_name(&short_filename, &short_ext) {
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

    pub fn read(
        &mut self,
        buffer: &mut [u8],
        offset: u32,
        table: AllocationTable,
        disk: &mut DiskAccess,
    ) -> u32 {
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
                }
                DirectoryType::Subdir(entry) => {
                    // needs to be implemented
                    return 0;
                }
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
        Self { dir_entry }
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

    pub fn get_modification_time(&self) -> u32 {
        self.dir_entry.get_modification_timestamp().as_u32()
    }

    pub fn read(
        &self,
        buffer: &mut [u8],
        initial_offset: u32,
        table: AllocationTable,
        disk: &mut DiskAccess,
    ) -> u32 {
        let mut offset = initial_offset;
        let mut bytes_written = 0;

        loop {
            let current_relative_cluster = offset / table.bytes_per_cluster();
            let cluster_offset = offset % table.bytes_per_cluster();

            let current_cluster = match table.get_nth_cluster(
                self.dir_entry.first_file_cluster as u32,
                current_relative_cluster,
                disk,
            ) {
                Some(cluster) => cluster,
                None => return bytes_written as u32,
            };
            let cluster_location = table.get_cluster_location(current_cluster);

            let bytes_remaining_in_file = self.byte_size() - offset;
            let bytes_remaining_in_cluster = table.bytes_per_cluster() - cluster_offset;

            let bytes_from_disk = bytes_remaining_in_file.min(bytes_remaining_in_cluster) as usize;
            let buffer_end = buffer.len().min(bytes_written + bytes_from_disk);

            let read_buffer = &mut buffer[bytes_written..buffer_end];

            let read_size =
                disk.read_bytes_from_disk(cluster_location + cluster_offset, read_buffer);
            bytes_written += read_size as usize;
            offset += read_size;

            if bytes_written as u32 + initial_offset >= self.byte_size() {
                // if there are no more bytes remaining in the file, exit early
                return bytes_written as u32;
            }
            if bytes_written >= buffer.len() {
                // if there is no more room in the buffer, exit
                return bytes_written as u32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DirEntry;

    #[test_case]
    fn filename_matching() {
        let mut direntry = DirEntry::new();
        &direntry.file_name.copy_from_slice("MYFILE  ".as_bytes());
        &direntry.ext.copy_from_slice("TXT".as_bytes());

        assert!(direntry.matches_name(
            &[b'M', b'Y', b'F', b'I', b'L', b'E', b' ', b' '],
            &[b'T', b'X', b'T'],
        ),);
        assert!(direntry.matches_name(
            &[b'M', b'y', b'F', b'i', b'l', b'e', b' ', b' '],
            &[b't', b'x', b't'],
        ),);

        assert!(!direntry.matches_name(
            &[b'O', b'T', b'H', b'E', b'R', b' ', b' ', b' '],
            &[b'T', b'X', b'T'],
        ),);
    }
}
