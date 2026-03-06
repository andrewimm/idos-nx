use alloc::vec::Vec;

/// Trait abstracting raw disk read/write operations.
/// On IDOS, this is backed by the block device via syscalls.
/// On host, this is backed by a file.
pub trait DiskIO {
    fn read(&mut self, buffer: &mut [u8], offset: u32) -> u32;
    fn write(&mut self, buffer: &[u8], offset: u32);
}

/// How many consecutive sectors to read ahead on a cache miss.
const READAHEAD_SECTORS: usize = 16;

/// Extra bytes reserved at the end of the buffer for readahead staging.
const STAGING_SIZE: usize = READAHEAD_SECTORS * 512;

/// Hash table size — must be a power of two, larger than max cache entries.
/// With 128 cache slots, 256 hash buckets gives ~50% load factor.
const HASH_TABLE_SIZE: usize = 256;
const HASH_EMPTY: u16 = 0xFFFF;

/// DiskAccess provides an easy read/write interface to the underlying disk. It
/// is responsible for fetching, caching, and flushing disk sectors. The rest
/// of the driver can treat the disk as a continuous byte stream, accessing
/// subsets of bytes at arbitrary offsets.
pub struct DiskAccess<D: DiskIO> {
    disk_io: D,
    /// Sector buffer — mmap'd directly from the kernel, not heap-allocated.
    buffer_ptr: *mut u8,
    buffer_size: usize,
    /// Per-slot metadata. Index matches buffer slot.
    cache_entries: Vec<CacheEntry>,
    /// Hash table: maps LBA → cache slot index. Uses open addressing.
    /// Stored as u16 indices (HASH_EMPTY = unused).
    hash_table: [u16; HASH_TABLE_SIZE],
    /// Global age counter — incremented on every access for LRU.
    global_age: u32,
}

fn hash_lba(lba: u32) -> usize {
    // Simple multiplicative hash
    ((lba.wrapping_mul(2654435761)) >> 16) as usize & (HASH_TABLE_SIZE - 1)
}

impl<D: DiskIO> DiskAccess<D> {
    pub fn new(disk_io: D, buffer_size: usize) -> Self {
        // Add space for readahead staging area after the cache slots
        let total_size = buffer_size + STAGING_SIZE;
        let page_aligned = (total_size + 0xFFF) & !0xFFF;

        // On IDOS, mmap directly from kernel to avoid heap pressure.
        // On host, fall back to a heap allocation.
        #[cfg(feature = "idos")]
        let buffer_ptr = {
            let vaddr = idos_api::syscall::memory::map_memory_contiguous(page_aligned as u32)
                .expect("Failed to mmap sector cache");
            vaddr as *mut u8
        };
        #[cfg(not(feature = "idos"))]
        let buffer_ptr = {
            let mut v = alloc::vec![0u8; page_aligned];
            let ptr = v.as_mut_ptr();
            core::mem::forget(v); // leak intentionally, we manage this memory
            ptr
        };

        Self {
            disk_io,
            buffer_ptr,
            buffer_size: page_aligned,
            cache_entries: Vec::new(),
            hash_table: [HASH_EMPTY; HASH_TABLE_SIZE],
            global_age: 0,
        }
    }

    /// Access the mmap'd buffer as a slice. Safe because the pointer is valid
    /// for the lifetime of this struct and we're single-threaded.
    fn buf(&self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.buffer_ptr, self.buffer_size) }
    }

    fn get_buffer_sector(&self, index: usize) -> &[u8] {
        let start = index * 512;
        let end = start + 512;
        &self.buf()[start..end]
    }

    fn get_max_cache_entries(&self) -> usize {
        // Exclude the staging area at the end
        (self.buffer_size - STAGING_SIZE) / 512
    }

    /// Get a mutable slice to the readahead staging area
    fn staging_area(&self) -> &mut [u8] {
        let offset = self.get_max_cache_entries() * 512;
        unsafe {
            core::slice::from_raw_parts_mut(
                self.buffer_ptr.add(offset),
                STAGING_SIZE,
            )
        }
    }

    /// O(1) amortized lookup of LBA in the hash table.
    fn hash_lookup(&self, lba: u32) -> Option<usize> {
        let mut idx = hash_lba(lba);
        for _ in 0..HASH_TABLE_SIZE {
            let slot = self.hash_table[idx];
            if slot == HASH_EMPTY {
                return None;
            }
            if self.cache_entries[slot as usize].lba == lba {
                return Some(slot as usize);
            }
            idx = (idx + 1) & (HASH_TABLE_SIZE - 1);
        }
        None
    }

    /// Insert an LBA → slot mapping into the hash table.
    fn hash_insert(&mut self, lba: u32, slot: usize) {
        let mut idx = hash_lba(lba);
        for _ in 0..HASH_TABLE_SIZE {
            if self.hash_table[idx] == HASH_EMPTY {
                self.hash_table[idx] = slot as u16;
                return;
            }
            idx = (idx + 1) & (HASH_TABLE_SIZE - 1);
        }
    }

    /// Remove an LBA from the hash table.
    fn hash_remove(&mut self, lba: u32) {
        let mut idx = hash_lba(lba);
        for _ in 0..HASH_TABLE_SIZE {
            let slot = self.hash_table[idx];
            if slot == HASH_EMPTY {
                return; // not found
            }
            if self.cache_entries[slot as usize].lba == lba {
                // Found — remove and rehash subsequent entries to fill the gap
                self.hash_table[idx] = HASH_EMPTY;
                let mut next = (idx + 1) & (HASH_TABLE_SIZE - 1);
                while self.hash_table[next] != HASH_EMPTY {
                    let rehash_slot = self.hash_table[next];
                    let rehash_lba = self.cache_entries[rehash_slot as usize].lba;
                    self.hash_table[next] = HASH_EMPTY;
                    self.hash_insert(rehash_lba, rehash_slot as usize);
                    next = (next + 1) & (HASH_TABLE_SIZE - 1);
                }
                return;
            }
            idx = (idx + 1) & (HASH_TABLE_SIZE - 1);
        }
    }

    fn cache_sector(&mut self, lba: u32) -> usize {
        self.global_age += 1;

        // O(1) lookup via hash table
        if let Some(slot) = self.hash_lookup(lba) {
            self.cache_entries[slot].last_access = self.global_age;
            return slot;
        }

        // Cache miss — read ahead multiple consecutive sectors
        self.readahead(lba);

        // The requested sector is now cached
        self.hash_lookup(lba).unwrap()
    }

    /// Read a batch of consecutive sectors starting at `start_lba`,
    /// populating cache slots for each. Skips sectors already cached.
    fn readahead(&mut self, start_lba: u32) {
        // Figure out how many consecutive uncached sectors to read
        let mut count = 0;
        for i in 0..READAHEAD_SECTORS {
            let lba = start_lba + i as u32;
            if self.hash_lookup(lba).is_some() {
                break; // stop at first already-cached sector
            }
            count += 1;
        }
        if count == 0 {
            return;
        }

        // Allocate cache slots for each sector (stack array, no heap alloc)
        let mut slots = [0usize; READAHEAD_SECTORS];
        for i in 0..count {
            let lba = start_lba + i as u32;
            slots[i] = self.allocate_slot(lba);
        }

        // Read all sectors into the staging area, then distribute into cache slots.
        let staging_offset = self.get_max_cache_entries() * 512;
        let read_size = count * 512;

        let staging = unsafe {
            core::slice::from_raw_parts_mut(self.buffer_ptr.add(staging_offset), STAGING_SIZE)
        };
        self.disk_io.read(&mut staging[..read_size], start_lba * 512);

        let buf = unsafe {
            core::slice::from_raw_parts_mut(self.buffer_ptr, self.buffer_size)
        };
        for i in 0..count {
            let src_start = staging_offset + i * 512;
            let dst_start = slots[i] * 512;
            buf.copy_within(src_start..src_start + 512, dst_start);
        }
    }

    /// Allocate a cache slot for the given LBA. Either grabs a free slot
    /// or evicts the least-recently-used entry.
    fn allocate_slot(&mut self, lba: u32) -> usize {
        let max_entries = self.get_max_cache_entries();

        if self.cache_entries.len() < max_entries {
            // Free slot available
            let index = self.cache_entries.len();
            self.cache_entries.push(CacheEntry {
                lba,
                last_access: self.global_age,
                dirty: false,
            });
            self.hash_insert(lba, index);
            index
        } else {
            // Evict the least recently used entry
            let mut oldest_idx = 0;
            let mut oldest_access = u32::MAX;
            for (i, entry) in self.cache_entries.iter().enumerate() {
                if entry.last_access < oldest_access {
                    oldest_idx = i;
                    oldest_access = entry.last_access;
                }
            }

            // Flush dirty sector before evicting
            if self.cache_entries[oldest_idx].dirty {
                let evict_lba = self.cache_entries[oldest_idx].lba;
                let mut flush_buf = [0u8; 512];
                flush_buf.copy_from_slice(self.get_buffer_sector(oldest_idx));
                self.disk_io.write(&flush_buf, evict_lba * 512);
            }

            // Remove old LBA from hash, insert new one
            let old_lba = self.cache_entries[oldest_idx].lba;
            self.hash_remove(old_lba);
            self.hash_insert(lba, oldest_idx);

            self.cache_entries[oldest_idx] = CacheEntry {
                lba,
                last_access: self.global_age,
                dirty: false,
            };
            oldest_idx
        }
    }

    pub fn read_bytes_from_disk(&mut self, offset: u32, buffer: &mut [u8]) -> u32 {
        let first_sector = offset / 512;
        let last_byte = offset + buffer.len() as u32;
        let last_sector = (last_byte + 511) / 512;
        let mut sector_offset = offset % 512;
        let mut to_read = buffer.len() as u32;
        let mut bytes_read = 0u32;

        for sector in first_sector..last_sector {
            let index = self.cache_sector(sector);
            let bytes_remaining_in_sector = 512 - sector_offset;
            let to_read_from_sector = to_read.min(bytes_remaining_in_sector);
            let src_start = index * 512 + sector_offset as usize;
            let dst_start = bytes_read as usize;
            buffer[dst_start..dst_start + to_read_from_sector as usize]
                .copy_from_slice(&self.buf()[src_start..src_start + to_read_from_sector as usize]);

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
        let first_sector = offset / 512;
        let last_byte = offset + data.len() as u32;
        let last_sector = (last_byte + 511) / 512;
        let mut sector_offset = offset % 512;
        let mut written = 0usize;

        for sector in first_sector..last_sector {
            let index = self.cache_sector(sector);
            self.cache_entries[index].dirty = true;
            let bytes_remaining_in_sector = 512 - sector_offset;
            let to_write = (data.len() - written).min(bytes_remaining_in_sector as usize);
            let dst_start = index * 512 + sector_offset as usize;
            self.buf()[dst_start..dst_start + to_write]
                .copy_from_slice(&data[written..written + to_write]);
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
    last_access: u32,
    dirty: bool,
}
