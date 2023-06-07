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

pub struct Directory {
}

pub struct File {
}

