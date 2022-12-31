#[repr(C, packed)]
#[allow(dead_code)]
pub struct FatHeader {
    pub jump_ops: [u8; 3],
    pub disk_name: [u8; 8],
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sector_count: u16,
    pub fat_count: u8,
    pub max_root_dir_entries: u16,
    pub total_sectors: u16,
    pub media_descriptor: u8,
    pub sectors_per_fat: u16,
    pub sectors_per_track: u16,
    pub head_count: u16,
}

#[repr(C, packed)]
#[allow(dead_code)]
pub struct DirectoryEntry {
    pub filename: [u8; 11],
    pub attributes: u8,
    pub extra_attributes: u8,
    pub create_time_fine: u8,
    pub create_time: u16,
    pub create_date: u16,
    pub last_access_date: u16,
    pub extended_attributes: u16,
    pub last_modify_time: u16,
    pub last_modify_date: u16,
    pub first_cluster: u16,
    pub file_size_bytes: u32,
}

pub struct DirectoryIterator {
    first_entry: *const DirectoryEntry,
    next_entry: usize,
    max_entries: usize,
}

impl DirectoryIterator {
    pub fn new(first_entry: *const DirectoryEntry) -> Self {
        Self {
            first_entry,
            next_entry: 0,
            max_entries: 16,
        }
    }
}

impl Iterator for DirectoryIterator {
    type Item = *const DirectoryEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_entry >= self.max_entries {
            return None;
        }
        let entry = unsafe {
            self.first_entry.offset(
                (core::mem::size_of::<DirectoryEntry>() * self.next_entry) as isize
            )
        };
        self.next_entry += 1;
        return Some(entry);
    }
}

pub fn find_root_dir_file(name: &str) -> Option<(u16, u32)> {
    let name_slice = name.as_bytes();
    let to_check = name_slice.len().min(11);

    let dir_entries = DirectoryIterator::new(0x7e00 as *const DirectoryEntry);
    for entry_ptr in dir_entries {
        let entry = unsafe { &*entry_ptr };
        if entry.filename[0] == 0 {
            return None;
        }
        if name_slice == &entry.filename[0..to_check] {
            crate::video::print_string("FOUND IT");
            return Some((entry.first_cluster, entry.file_size_bytes));
        }
    }

    None
}
