use alloc::vec::Vec;

/// Trait abstracting raw disk read/write operations.
/// On IDOS, this is backed by the block device via syscalls.
/// On host, this is backed by a file.
pub trait DiskIO {
    fn read(&mut self, buffer: &mut [u8], offset: u32) -> u32;
    fn write(&mut self, buffer: &[u8], offset: u32);
}

/// DiskAccess provides an easy read/write interface to the underlying disk. It
/// is responsible for fetching, caching, and flushing disk sectors. The rest
/// of the driver can treat the disk as a continuous byte stream, accessing
/// subsets of bytes at arbitrary offsets.
pub struct DiskAccess<D: DiskIO> {
    disk_io: D,
    buffer: Vec<u8>,
    /// Obviously LRU is *not* the best caching strategy, since you'd also want
    /// to account for frequency of hits. However, it's good enough for now!
    cache_entries: Vec<CacheEntry>,
}

impl<D: DiskIO> DiskAccess<D> {
    pub fn new(disk_io: D, buffer_size: usize) -> Self {
        let buffer = alloc::vec![0u8; buffer_size];

        Self {
            disk_io,
            buffer,
            cache_entries: Vec::new(),
        }
    }

    fn get_buffer_sector(&self, index: usize) -> &[u8] {
        let start = index * 512;
        let end = start + 512;
        &self.buffer[start..end]
    }

    fn get_max_cache_entries(&self) -> usize {
        self.buffer.len() / 512
    }

    fn cache_sector(&mut self, lba: u32) -> usize {
        let mut found = None;
        for (index, entry) in self.cache_entries.iter_mut().enumerate() {
            if entry.lba == lba {
                entry.age = 0;
                found = Some(index);
            } else {
                entry.age += 1;
            }
        }
        if let Some(index) = found {
            return index;
        }
        let cache_index = if self.cache_entries.len() < self.get_max_cache_entries() {
            for entry in self.cache_entries.iter_mut() {
                entry.age += 1;
            }
            let index = self.cache_entries.len();
            self.cache_entries.push(CacheEntry { lba, age: 0, dirty: false });
            index
        } else {
            // need to evict an entry
            let mut oldest: (usize, u32) = (0, 0);
            for (index, entry) in self.cache_entries.iter().enumerate() {
                if entry.age > oldest.1 {
                    oldest.0 = index;
                    oldest.1 = entry.age;
                }
            }
            // flush dirty sector before evicting
            if self.cache_entries[oldest.0].dirty {
                let evict_lba = self.cache_entries[oldest.0].lba;
                let mut flush_buf = [0u8; 512];
                flush_buf.copy_from_slice(self.get_buffer_sector(oldest.0));
                self.disk_io.write(&flush_buf, evict_lba * 512);
            }
            for (index, entry) in self.cache_entries.iter_mut().enumerate() {
                if index == oldest.0 {
                    entry.age = 0;
                    entry.lba = lba;
                    entry.dirty = false;
                    break;
                }
            }
            oldest.0
        };
        let start = cache_index * 512;
        let end = start + 512;
        self.disk_io.read(&mut self.buffer[start..end], lba * 512);
        cache_index
    }

    pub fn read_bytes_from_disk(&mut self, offset: u32, buffer: &mut [u8]) -> u32 {
        let sectors = sectors_for_byte_range(offset, buffer.len());
        let mut sector_offset = offset % 512;
        let mut to_read = buffer.len() as u32;
        let mut bytes_read = 0;
        for sector in sectors {
            let index = self.cache_sector(sector);
            let bytes_remaining_in_sector = 512 - sector_offset;
            let to_read_from_sector = to_read.min(bytes_remaining_in_sector);
            for i in 0..to_read_from_sector {
                buffer[(bytes_read + i) as usize] = self.buffer[index * 512 + sector_offset as usize + i as usize];
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

    pub fn write_bytes_to_disk(&mut self, offset: u32, data: &[u8]) {
        let sectors = sectors_for_byte_range(offset, data.len());
        let mut sector_offset = offset % 512;
        let mut written = 0usize;
        for sector in sectors {
            let index = self.cache_sector(sector);
            self.cache_entries[index].dirty = true;
            let bytes_remaining_in_sector = 512 - sector_offset;
            let to_write = (data.len() - written).min(bytes_remaining_in_sector as usize);
            for i in 0..to_write {
                self.buffer[index * 512 + (sector_offset as usize) + i] = data[written + i];
            }
            written += to_write;
            sector_offset = 0;
        }
    }

    pub fn write_struct_to_disk<S: Sized>(&mut self, offset: u32, s: &S) {
        let buffer_ptr = s as *const S as *const u8;
        let buffer_size = core::mem::size_of::<S>();
        let buffer = unsafe { core::slice::from_raw_parts(buffer_ptr, buffer_size) };
        self.write_bytes_to_disk(offset, buffer);
    }

    pub fn flush_all(&mut self) {
        for index in 0..self.cache_entries.len() {
            if self.cache_entries[index].dirty {
                let lba = self.cache_entries[index].lba;
                let mut flush_buf = [0u8; 512];
                flush_buf.copy_from_slice(self.get_buffer_sector(index));
                self.disk_io.write(&flush_buf, lba * 512);
                self.cache_entries[index].dirty = false;
            }
        }
    }
}

pub struct CacheEntry {
    lba: u32,
    age: u32,
    dirty: bool,
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
