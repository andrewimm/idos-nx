use crate::io::handle::Handle;
use crate::memory::address::VirtualAddress;
use crate::task::actions::handle::create_file_handle;
use crate::task::actions::io::{open_sync, read_sync};
use crate::task::actions::memory::map_memory;
use crate::task::memory::MemoryBacking;
use alloc::vec::Vec;

/// DiskAccess provides an easy read/write interface to the underlying disk. It
/// is responsible for fetching, caching, and flushing disk sectors. The rest
/// of the driver can treat the disk as a continuous byte stream, accessing
/// subsets of bytes at arbitrary offsets.
pub struct DiskAccess {
    mount_handle: Handle,
    buffer_location: VirtualAddress,
    buffer_size: usize,
    /// Obviously LRU is *not* the best caching strategy, since you'd also want
    /// to account for frequency of hits. However, it's good enough for now!
    cache_entries: Vec<CacheEntry>,
}

impl DiskAccess {
    pub fn new(mount: &str) -> Self {
        let mount_handle = create_file_handle();

        open_sync(mount_handle, mount).unwrap();

        let buffer_size = 4096;

        let buffer_location =
            map_memory(None, buffer_size as u32, MemoryBacking::Anonymous).unwrap();

        let disk = Self {
            mount_handle,
            buffer_location,
            buffer_size,
            cache_entries: Vec::new(),
        };

        // zero out the cache
        let cache = disk.get_buffer();
        for i in 0..cache.len() {
            cache[i] = 0;
        }

        disk
    }

    fn get_buffer(&self) -> &mut [u8] {
        let data = self.buffer_location.as_u32() as *mut u8;
        let len = self.buffer_size;
        unsafe { core::slice::from_raw_parts_mut(data, len) }
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
        let mut found = None;
        for (index, entry) in self.cache_entries.iter_mut().enumerate() {
            if entry.lba == lba {
                // already cached!
                entry.age = 0;
                found = Some(index);
            } else {
                entry.age += 1;
            }
        }
        if let Some(index) = found {
            return index;
        }
        crate::kprintln!("FAT CACHE MISS");
        let cache_index = if self.cache_entries.len() < self.get_max_cache_entries() {
            for entry in self.cache_entries.iter_mut() {
                entry.age += 1;
            }
            let index = self.cache_entries.len();
            self.cache_entries.push(CacheEntry { lba, age: 0 });
            index
        } else {
            // need to evict an entry
            //              (index, age)
            let mut oldest: (usize, u32) = (0, 0);
            for (index, entry) in self.cache_entries.iter().enumerate() {
                if entry.age > oldest.1 {
                    oldest.0 = index;
                    oldest.1 = entry.age;
                }
            }
            for (index, entry) in self.cache_entries.iter_mut().enumerate() {
                if index == oldest.0 {
                    entry.age = 0;
                    entry.lba = lba;
                    break;
                }
            }
            oldest.0
        };
        let cache_buffer = self.get_buffer_sector(cache_index);

        read_sync(self.mount_handle, cache_buffer, lba * 512).unwrap();
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
            to_read -= to_read_from_sector;
            sector_offset = 0;
        }

        bytes_read
    }

    pub fn read_struct_from_disk<S: Sized>(&mut self, offset: u32, s: &mut S) {
        let buffer_ptr = s as *mut S as *mut u8;
        let buffer_size = core::mem::size_of::<S>();
        let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr, buffer_size) };
        self.read_bytes_from_disk(offset, buffer);
    }
}

pub struct CacheEntry {
    lba: u32,
    age: u32,
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
