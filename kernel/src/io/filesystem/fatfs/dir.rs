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

    pub fn set_size(&mut self, size: u32) {
        self.byte_size = size;
    }

    pub fn set_first_cluster(&mut self, cluster: u16) {
        self.first_file_cluster = cluster;
    }

    pub fn set_filename(&mut self, filename: &[u8; 8], ext: &[u8; 3]) {
        self.file_name = *filename;
        self.ext = *ext;
    }

    pub fn set_attributes(&mut self, attributes: u8) {
        self.attributes = attributes;
    }

    pub fn first_file_cluster(&self) -> u16 {
        self.first_file_cluster
    }

    pub fn mark_deleted(&mut self) {
        self.file_name[0] = 0xE5;
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
    max_entries: u32,
}

impl RootDirectory {
    pub fn new(first_sector: u32, max_entries: u32) -> Self {
        Self {
            first_sector,
            max_entries,
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
            max_index: self.max_entries,
            current,
        }
    }

    /// Add a new directory entry to the root directory.
    /// Returns the disk offset of the newly written entry.
    pub fn add_entry(
        &self,
        filename: &[u8; 8],
        ext: &[u8; 3],
        attributes: u8,
        first_cluster: u16,
        disk: &mut DiskAccess,
    ) -> Option<u32> {
        let dir_offset = self.first_sector * 512;
        let entry_size = core::mem::size_of::<DirEntry>() as u32;

        for i in 0..self.max_entries {
            let offset = dir_offset + i * entry_size;
            let mut entry = DirEntry::new();
            disk.read_struct_from_disk(offset, &mut entry);

            // A slot is free if first byte is 0x00 (end of directory) or 0xE5 (deleted)
            if entry.file_name[0] == 0x00 || entry.file_name[0] == 0xE5 {
                let mut new_entry = DirEntry::new();
                new_entry.set_filename(filename, ext);
                new_entry.set_attributes(attributes);
                new_entry.set_first_cluster(first_cluster);
                disk.write_struct_to_disk(offset, &new_entry);
                return Some(offset);
            }
        }
        None // no free slots
    }

    /// Remove a directory entry by name. Sets the first byte to 0xE5 (deleted marker).
    /// Returns the DirEntry that was removed (for cluster chain cleanup).
    pub fn remove_entry(
        &self,
        filename: &[u8; 8],
        ext: &[u8; 3],
        disk: &mut DiskAccess,
    ) -> Option<DirEntry> {
        let dir_offset = self.first_sector * 512;
        let entry_size = core::mem::size_of::<DirEntry>() as u32;

        for i in 0..self.max_entries {
            let offset = dir_offset + i * entry_size;
            let mut entry = DirEntry::new();
            disk.read_struct_from_disk(offset, &mut entry);

            if entry.file_name[0] == 0x00 {
                return None; // end of directory
            }
            if entry.file_name[0] == 0xE5 {
                continue; // deleted entry
            }

            if entry.matches_name(filename, ext) {
                let removed = entry;
                entry.mark_deleted();
                disk.write_struct_to_disk(offset, &entry);
                return Some(removed);
            }
        }
        None
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

        for (entry, disk_offset) in self.iter(disk) {
            if entry.matches_name(&short_filename, &short_ext) {
                if entry.is_directory() {
                    return Some(Entity::Dir(Directory::from_dir_entry(entry)));
                } else {
                    return Some(Entity::File(File::from_dir_entry(entry, disk_offset)));
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
    max_index: u32,
    current: DirEntry,
}

impl RootDirectoryIter<'_> {
    /// Returns the disk offset of the entry that will be returned by the next call to `next()`.
    fn current_entry_offset(&self) -> u32 {
        self.dir_offset + self.current_index * core::mem::size_of::<DirEntry>() as u32
    }
}

impl Iterator for RootDirectoryIter<'_> {
    type Item = (DirEntry, u32);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_empty() {
            return None;
        }

        if self.current_index + 1 >= self.max_index {
            return None;
        }

        let entry = self.current.clone();
        let entry_offset = self.current_entry_offset();
        self.current_index += 1;
        let offset = self.dir_offset + self.current_index * core::mem::size_of::<DirEntry>() as u32;
        self.disk.read_struct_from_disk(offset, &mut self.current);

        Some((entry, entry_offset))
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
    pub fn dir_type(&self) -> &DirectoryType {
        &self.dir_type
    }

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
        _table: AllocationTable,
        disk: &mut DiskAccess,
    ) -> u32 {
        if !self.entries_fetched {
            // the first time the directory IO handle is read, it caches the
            // entries it contains
            match &self.dir_type {
                DirectoryType::Root(root) => {
                    for (entry, _offset) in root.iter(disk) {
                        let name = entry.get_full_name();
                        self.entries.extend_from_slice(name.as_bytes());
                        self.entries.push(0);
                    }
                    self.entries_fetched = true;
                }
                DirectoryType::Subdir(_entry) => {
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

#[derive(Clone)]
pub struct File {
    dir_entry: DirEntry,
    dir_entry_disk_offset: u32,
    cluster_cache: Vec<u32>,
}

impl File {
    pub fn from_dir_entry(dir_entry: DirEntry, disk_offset: u32) -> Self {
        Self {
            dir_entry,
            dir_entry_disk_offset: disk_offset,
            cluster_cache: Vec::new(),
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

    pub fn first_cluster(&self) -> u16 {
        self.dir_entry.first_file_cluster
    }

    pub fn dir_entry_disk_offset(&self) -> u32 {
        self.dir_entry_disk_offset
    }

    pub fn dir_entry_mut(&mut self) -> &mut DirEntry {
        &mut self.dir_entry
    }

    pub fn invalidate_cluster_cache(&mut self) {
        self.cluster_cache.clear();
    }

    pub fn get_modification_time(&self) -> u32 {
        self.dir_entry.get_modification_timestamp().as_u32()
    }

    pub fn cache_cluster_chain(
        &mut self,
        table: AllocationTable,
        start_cluster: u32,
        disk: &mut DiskAccess,
    ) {
        self.cluster_cache.clear();
        let mut current_cluster = start_cluster;
        while current_cluster != 0xfff {
            self.cluster_cache.push(current_cluster);
            current_cluster = match table.get_next_cluster(current_cluster, disk) {
                Some(next) => next,
                None => return,
            }
        }
    }

    pub fn write(
        &mut self,
        data: &[u8],
        initial_offset: u32,
        table: AllocationTable,
        disk: &mut DiskAccess,
    ) -> u32 {
        let mut offset = initial_offset;
        let mut bytes_written = 0usize;

        // Ensure cluster chain is cached
        if self.cluster_cache.is_empty() && self.dir_entry.first_file_cluster != 0 {
            self.cache_cluster_chain(table, self.dir_entry.first_file_cluster as u32, disk);
        }

        // If file has no clusters yet, allocate the first one
        if self.cluster_cache.is_empty() {
            if let Some(cluster) = table.allocate_cluster(disk) {
                self.dir_entry.first_file_cluster = cluster as u16;
                self.cluster_cache.push(cluster);
            } else {
                return 0;
            }
        }

        loop {
            if bytes_written >= data.len() {
                break;
            }

            let current_relative_cluster = offset / table.bytes_per_cluster();
            let cluster_offset = offset % table.bytes_per_cluster();

            // Extend chain if needed
            while current_relative_cluster as usize >= self.cluster_cache.len() {
                let prev_cluster = *self.cluster_cache.last().unwrap();
                if let Some(new_cluster) = table.allocate_cluster(disk) {
                    // Link previous cluster to new one
                    table.set_cluster_entry(prev_cluster, new_cluster, disk);
                    self.cluster_cache.push(new_cluster);
                } else {
                    // Disk full
                    return bytes_written as u32;
                }
            }

            let current_cluster = self.cluster_cache[current_relative_cluster as usize];
            let cluster_location = table.get_cluster_location(current_cluster);

            let bytes_remaining_in_cluster = table.bytes_per_cluster() - cluster_offset;
            let bytes_to_write = (data.len() - bytes_written).min(bytes_remaining_in_cluster as usize);

            disk.write_bytes_to_disk(
                cluster_location + cluster_offset,
                &data[bytes_written..bytes_written + bytes_to_write],
            );

            bytes_written += bytes_to_write;
            offset += bytes_to_write as u32;
        }

        // Update file size if we wrote beyond the end
        let new_end = initial_offset + bytes_written as u32;
        if new_end > self.dir_entry.byte_size {
            self.dir_entry.byte_size = new_end;
            // Write updated dir entry back to disk
            disk.write_struct_to_disk(self.dir_entry_disk_offset, &self.dir_entry);
        }

        bytes_written as u32
    }

    pub fn read(
        &mut self,
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

            if self.cluster_cache.is_empty() {
                self.cache_cluster_chain(table, self.dir_entry.first_file_cluster as u32, disk);
            }

            let current_cluster = match self.cluster_cache.get(current_relative_cluster as usize) {
                Some(&cluster) => cluster,
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

/// Parse a filename string into FAT 8.3 format (uppercase, space-padded).
pub fn parse_short_name(name: &str) -> ([u8; 8], [u8; 3]) {
    let (filename, ext) = match name.rsplit_once('.') {
        Some(pair) => pair,
        None => (name, ""),
    };
    let mut short_filename: [u8; 8] = [0x20; 8];
    let mut short_ext: [u8; 3] = [0x20; 3];
    let filename_len = filename.len().min(8);
    let ext_len = ext.len().min(3);
    for i in 0..filename_len {
        short_filename[i] = filename.as_bytes()[i].to_ascii_uppercase();
    }
    for i in 0..ext_len {
        short_ext[i] = ext.as_bytes()[i].to_ascii_uppercase();
    }
    (short_filename, short_ext)
}

/// Check if a subdirectory (given by its first cluster) is empty.
pub fn is_subdir_empty(
    first_cluster: u32,
    table: &AllocationTable,
    disk: &mut DiskAccess,
) -> bool {
    let cluster_location = table.get_cluster_location(first_cluster);
    let entry_size = core::mem::size_of::<DirEntry>() as u32;
    let entries_per_cluster = table.bytes_per_cluster() / entry_size;

    let mut cluster = first_cluster;
    loop {
        let loc = table.get_cluster_location(cluster);
        for i in 0..entries_per_cluster {
            let offset = loc + i * entry_size;
            let mut entry = DirEntry::new();
            disk.read_struct_from_disk(offset, &mut entry);

            if entry.file_name[0] == 0x00 {
                return true; // end of directory, no entries found
            }
            if entry.file_name[0] == 0xE5 {
                continue; // deleted entry
            }
            // Skip . and .. entries
            if entry.get_filename() == "." || entry.get_filename() == ".." {
                continue;
            }
            return false; // found a real entry
        }
        match table.get_next_cluster(cluster, disk) {
            Some(next) => cluster = next,
            None => return true,
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
