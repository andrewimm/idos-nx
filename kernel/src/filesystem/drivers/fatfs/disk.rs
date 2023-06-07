use alloc::vec::Vec;
use crate::memory::address::VirtualAddress;
use crate::task::actions::io::{open_path, read_file};
use crate::task::actions::memory::map_memory;
use crate::task::files::FileHandle;
use crate::task::memory::MemoryBacking;

/// DiskAccess provides an easy read/write interface to the underlying disk. It
/// is responsible for fetching, caching, and flushing disk sectors. The rest
/// of the driver can treat the disk as a continuous byte stream, accessing
/// subsets of bytes at arbitrary offsets.
pub struct DiskAccess {
    mount_handle: FileHandle,
    buffer_location: VirtualAddress,
    buffer_size: usize,
    cache_entries: Vec<CacheEntry>,
}

impl DiskAccess {
    pub fn new(mount: &str) -> Self {
        let mount_handle = open_path(mount).unwrap();

        let buffer_size = 4096;

        let buffer_location = map_memory(None, buffer_size as u32, MemoryBacking::Anonymous).unwrap();

        Self {
            mount_handle,
            buffer_location,
            buffer_size,
            cache_entries: Vec::new(),
        }
    }

    fn get_buffer(&self) -> &mut [u8] {
        let data = self.buffer_location.as_u32() as *mut u8;
        let len = self.buffer_size;
        unsafe {
            core::slice::from_raw_parts_mut(data, len)
        }
    }

    fn get_buffer_sector(&self, index: usize) -> &mut [u8] {
        let start = index * 512;
        let end = start + 512;
        let buffer = self.get_buffer();
        &mut buffer[start..end]
    }

    fn get_max_cache_entries(&self) -> usize {
        self.buffer_size / 512
    }

    fn cache_sector(&mut self, lba: u32) -> usize {
        for (index, entry) in self.cache_entries.iter().enumerate() {
            if entry.lba == lba {
                // already cached!
                return index;
            }
        }
        let cache_index = if self.cache_entries.len() < self.get_max_cache_entries() {
            let index = self.cache_entries.len();
            self.cache_entries.push(
                CacheEntry {
                    lba,
                }
            );
            let cache_buffer = self.get_buffer_sector(index);
            read_file(self.mount_handle, cache_buffer).unwrap();

            index
        } else {
            // need to evict an entry
            0
        };
        cache_index
    }

    pub fn read_bytes_from_disk(&mut self, offset: u32, buffer: &mut [u8]) -> u32 {
        let sectors = sectors_for_byte_range(offset, buffer.len());
        let mut sector_offset = offset % 512;
        let mut to_read = buffer.len() as u32;
        let mut bytes_read = 0;
        for sector in sectors {
            let index = self.cache_sector(sector);
            let disk_buffer = self.get_buffer_sector(index);
            let bytes_remaining_in_sector = 512 - sector_offset;
            let to_read_from_sector = to_read.min(bytes_remaining_in_sector);
            for i in 0..to_read_from_sector {
                buffer[(bytes_read + i) as usize] = disk_buffer[(sector_offset + i) as usize];
            }

            bytes_read += to_read_from_sector;
            sector_offset = 0;
        }

        bytes_read
    }
}

pub struct CacheEntry {
    lba: u32,
}

fn sectors_for_byte_range(offset: u32, length: usize) -> Vec<u32> {
    let mut sectors = Vec::new();
    let end = offset + length as u32;
    let mut cursor = (offset / 512) * 512;
    while cursor < end {
        sectors.push(cursor / 512);
        cursor += 512;
    }

    sectors
}
