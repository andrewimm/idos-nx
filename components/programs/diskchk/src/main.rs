#![no_std]
#![no_main]

extern crate idos_api;
extern crate idos_sdk;

use idos_api::io::sync::{close_sync, open_sync, read_sync, write_sync};
use idos_api::io::Handle;
use idos_api::syscall::io::create_file_handle;
use idos_api::syscall::memory::map_memory;

const STDOUT: Handle = Handle::new(1);
const PAGE_SIZE: usize = 0x1000;

// ---------------------------------------------------------------------------
// Output buffer
// ---------------------------------------------------------------------------

struct Buf {
    ptr: *mut u8,
    len: usize,
    cap: usize,
}

impl Buf {
    fn new(pages: usize) -> Self {
        let cap = pages * PAGE_SIZE;
        let addr = map_memory(None, cap as u32, None).unwrap();
        Self { ptr: addr as *mut u8, len: 0, cap }
    }

    fn flush(&mut self) {
        if self.len > 0 {
            let _ = write_sync(STDOUT, unsafe { core::slice::from_raw_parts(self.ptr, self.len) }, 0);
            self.len = 0;
        }
    }

    fn push(&mut self, s: &[u8]) {
        for &b in s {
            self.push_byte(b);
        }
    }

    fn push_byte(&mut self, b: u8) {
        if self.len >= self.cap {
            self.flush();
        }
        unsafe { *self.ptr.add(self.len) = b; }
        self.len += 1;
    }

    fn push_u32(&mut self, mut v: u32, width: usize) {
        let mut tmp = [0u8; 10];
        let mut pos = 0;
        if v == 0 {
            tmp[0] = b'0';
            pos = 1;
        } else {
            while v > 0 {
                tmp[pos] = b'0' + (v % 10) as u8;
                v /= 10;
                pos += 1;
            }
        }
        // right-align in width
        if pos < width {
            for _ in 0..(width - pos) {
                self.push_byte(b' ');
            }
        }
        for i in (0..pos).rev() {
            self.push_byte(tmp[i]);
        }
    }

    fn push_str(&mut self, s: &str) {
        self.push(s.as_bytes());
    }
}

// ---------------------------------------------------------------------------
// On-disk structures
// ---------------------------------------------------------------------------

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct BiosParamBlock {
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    fat_count: u8,
    root_directory_entries: u16,
    total_sectors: u16,
    media_descriptor: u8,
    sectors_per_fat: u16,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct DirEntry {
    file_name: [u8; 8],
    ext: [u8; 3],
    attributes: u8,
    _reserved: [u8; 14],
    first_file_cluster: u16,
    byte_size: u32,
}

const DIR_ATTR_VOLUME_LABEL: u8 = 0x08;
const DIR_ATTR_LONG_NAME: u8 = 0x0F;
const DIR_ATTR_DIRECTORY: u8 = 0x10;

// ---------------------------------------------------------------------------
// FAT12 helpers
// ---------------------------------------------------------------------------

fn get_fat_entry(fat_data: &[u8], cluster: u32) -> u16 {
    let pair_offset = (cluster / 2 * 3) as usize;
    if pair_offset + 2 >= fat_data.len() {
        return 0;
    }
    let triple = (fat_data[pair_offset] as u32)
        | ((fat_data[pair_offset + 1] as u32) << 8)
        | ((fat_data[pair_offset + 2] as u32) << 16);
    if cluster & 1 == 0 {
        (triple & 0xFFF) as u16
    } else {
        ((triple >> 12) & 0xFFF) as u16
    }
}

fn is_eof(val: u16) -> bool {
    val >= 0xFF8
}

fn is_free(val: u16) -> bool {
    val == 0x000
}

fn is_bad(val: u16) -> bool {
    val == 0xFF7
}

// ---------------------------------------------------------------------------
// Disk reader helper
// ---------------------------------------------------------------------------

struct DiskReader {
    handle: Handle,
    sector_buf: *mut u8,
}

const MAX_DMA_READ: usize = 8 * 512; // floppy DMA buffer is one page (8 sectors)

impl DiskReader {
    fn new(handle: Handle) -> Self {
        let addr = map_memory(None, 512, None).unwrap();
        Self { handle, sector_buf: addr as *mut u8 }
    }

    /// Read `buf.len()` bytes at `offset`, chunking to stay within DMA limits.
    fn read_bytes(&self, buf: &mut [u8], offset: u32) -> bool {
        let mut pos = 0usize;
        while pos < buf.len() {
            let remaining = buf.len() - pos;
            let chunk = if remaining > MAX_DMA_READ { MAX_DMA_READ } else { remaining };
            if read_sync(self.handle, &mut buf[pos..pos + chunk], offset + pos as u32).is_err() {
                return false;
            }
            pos += chunk;
        }
        true
    }

    fn read_sector(&self, lba: u32) -> &[u8] {
        let buf = unsafe { core::slice::from_raw_parts_mut(self.sector_buf, 512) };
        let _ = read_sync(self.handle, buf, lba * 512);
        unsafe { core::slice::from_raw_parts(self.sector_buf, 512) }
    }
}

// ---------------------------------------------------------------------------
// Allocated buffer helper
// ---------------------------------------------------------------------------

fn alloc_buf(size: usize) -> *mut u8 {
    let pages = (size + PAGE_SIZE - 1) / PAGE_SIZE;
    let total = pages * PAGE_SIZE;
    map_memory(None, total as u32, None).unwrap() as *mut u8
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn main() {
    let mut out = Buf::new(2);

    out.push(b"DISKCHK - FAT12 Filesystem Checker\n\n");

    // Parse arguments
    let mut args = idos_sdk::env::args();
    let _prog = args.next(); // skip program name
    let dev_path = match args.next() {
        Some(p) => p,
        None => {
            out.push(b"Usage: DISKCHK DEV:\\FD1\n");
            out.flush();
            return;
        }
    };

    // Open device
    let handle = create_file_handle();
    if open_sync(handle, dev_path).is_err() {
        out.push(b"Error: cannot open device ");
        out.push_str(dev_path);
        out.push(b"\n");
        out.flush();
        return;
    }

    let disk = DiskReader::new(handle);
    let mut errors: u32 = 0;

    // -----------------------------------------------------------------------
    // 1. Read and validate BPB
    // -----------------------------------------------------------------------
    out.push(b"Reading boot sector...\n");

    let boot = disk.read_sector(0);
    // BPB starts at byte offset 0x0B
    let bpb: BiosParamBlock = unsafe {
        core::ptr::read_unaligned(boot.as_ptr().add(0x0B) as *const BiosParamBlock)
    };

    let bytes_per_sector = bpb.bytes_per_sector;
    let sectors_per_cluster = bpb.sectors_per_cluster;
    let reserved_sectors = bpb.reserved_sectors;
    let fat_count = bpb.fat_count;
    let root_dir_entries = bpb.root_directory_entries;
    let total_sectors = bpb.total_sectors;
    let sectors_per_fat = bpb.sectors_per_fat;

    out.push(b"  Bytes/sector:    "); out.push_u32(bytes_per_sector as u32, 0); out.push(b"\n");
    out.push(b"  Sectors/cluster: "); out.push_u32(sectors_per_cluster as u32, 0); out.push(b"\n");
    out.push(b"  Reserved sectors: "); out.push_u32(reserved_sectors as u32, 0); out.push(b"\n");
    out.push(b"  FAT copies:      "); out.push_u32(fat_count as u32, 0); out.push(b"\n");
    out.push(b"  Sectors/FAT:     "); out.push_u32(sectors_per_fat as u32, 0); out.push(b"\n");
    out.push(b"  Root dir entries: "); out.push_u32(root_dir_entries as u32, 0); out.push(b"\n");
    out.push(b"  Total sectors:   "); out.push_u32(total_sectors as u32, 0); out.push(b"\n");

    // Validate BPB
    if bytes_per_sector != 512 {
        out.push(b"  ERROR: bytes_per_sector != 512\n");
        errors += 1;
    }
    if fat_count < 1 {
        out.push(b"  ERROR: fat_count < 1\n");
        errors += 1;
    }
    if sectors_per_fat == 0 {
        out.push(b"  ERROR: sectors_per_fat == 0\n");
        errors += 1;
    }
    if sectors_per_cluster == 0 || (sectors_per_cluster & (sectors_per_cluster - 1)) != 0 {
        out.push(b"  ERROR: sectors_per_cluster not power of 2\n");
        errors += 1;
    }

    if errors > 0 {
        out.push(b"\nBPB validation failed, cannot continue.\n");
        out.push(b"\n  "); out.push_u32(errors, 0); out.push(b" errors found\n");
        out.flush();
        let _ = close_sync(handle);
        return;
    }

    out.push(b"\n");

    // Compute geometry
    let fat_start = (reserved_sectors as u32) * 512;
    let fat_size = (sectors_per_fat as u32) * 512;
    let root_dir_start = fat_start + (fat_count as u32) * fat_size;
    let root_dir_size = (root_dir_entries as u32) * 32;
    let root_dir_sectors = (root_dir_size + 511) / 512;
    let _data_start = root_dir_start + root_dir_sectors * 512;
    let total_data_sectors = (total_sectors as u32) - (reserved_sectors as u32)
        - (fat_count as u32) * (sectors_per_fat as u32) - root_dir_sectors;
    let total_clusters = total_data_sectors / (sectors_per_cluster as u32);
    let max_cluster = total_clusters + 1; // clusters numbered 2..=max_cluster
    let bytes_per_cluster = (sectors_per_cluster as u32) * 512;

    // -----------------------------------------------------------------------
    // 2. Read FAT data and check FAT copy consistency
    // -----------------------------------------------------------------------
    out.push(b"Checking FAT copies...");

    // Read FAT 1
    let fat1_ptr = alloc_buf(fat_size as usize);
    let fat1 = unsafe { core::slice::from_raw_parts_mut(fat1_ptr, fat_size as usize) };
    disk.read_bytes(fat1, fat_start);

    if fat_count >= 2 {
        // Read FAT 2 and compare
        let fat2_ptr = alloc_buf(fat_size as usize);
        let fat2 = unsafe { core::slice::from_raw_parts_mut(fat2_ptr, fat_size as usize) };
        disk.read_bytes(fat2, fat_start + fat_size);

        let mut mismatch_count: u32 = 0;
        for i in 0..fat_size as usize {
            if fat1[i] != fat2[i] {
                if mismatch_count == 0 {
                    out.push(b" MISMATCH\n");
                }
                if mismatch_count < 10 {
                    out.push(b"  FAT1 != FAT2 at byte offset ");
                    out.push_u32(i as u32, 0);
                    out.push(b"\n");
                }
                mismatch_count += 1;
            }
        }

        if mismatch_count > 0 {
            if mismatch_count > 10 {
                out.push(b"  ... and ");
                out.push_u32(mismatch_count - 10, 0);
                out.push(b" more mismatches\n");
            }
            errors += mismatch_count;
        } else {
            out.push(b" OK\n");
        }
    } else {
        out.push(b" OK (single copy)\n");
    }

    // -----------------------------------------------------------------------
    // 3. FAT chain integrity + build used_by array
    // -----------------------------------------------------------------------
    out.push(b"Checking FAT chains...");

    // used_by[cluster - 2] = file entry index that references it (1-based), 0 = unused
    let used_by_size = (total_clusters as usize) * 2;
    let used_by_ptr = alloc_buf(used_by_size) as *mut u16;
    let used_by = unsafe { core::slice::from_raw_parts_mut(used_by_ptr, total_clusters as usize) };
    // zero-init
    for i in 0..total_clusters as usize {
        used_by[i] = 0;
    }

    let mut chain_errors: u32 = 0;
    let mut cross_links: u32 = 0;

    // We'll fill used_by during directory scan; for now check all chains for loops
    // by scanning every allocated cluster in the FAT
    // Check for values pointing out of range
    for c in 2..=max_cluster {
        let val = get_fat_entry(fat1, c);
        if !is_free(val) && !is_eof(val) && !is_bad(val) {
            if (val as u32) < 2 || (val as u32) > max_cluster {
                if chain_errors < 10 {
                    out.push(b"\n  Cluster ");
                    out.push_u32(c, 0);
                    out.push(b" points to invalid cluster ");
                    out.push_u32(val as u32, 0);
                }
                chain_errors += 1;
            }
        }
    }

    if chain_errors > 0 {
        out.push(b"\n");
        errors += chain_errors;
    } else {
        out.push(b" OK\n");
    }

    // -----------------------------------------------------------------------
    // Helper: follow a chain and mark used_by, detect cross-links and loops
    // -----------------------------------------------------------------------
    // owner_id: 1-based identifier for the file/dir that owns this chain
    // Returns: cluster count in chain
    fn follow_chain(
        fat: &[u8],
        used_by: &mut [u16],
        start: u16,
        owner_id: u16,
        max_cluster: u32,
        errors: &mut u32,
        cross_links: &mut u32,
        out: &mut Buf,
    ) -> u32 {
        if start < 2 || (start as u32) > max_cluster {
            return 0;
        }

        let mut count: u32 = 0;
        let mut current = start as u32;
        // visited set via tortoise-and-hare is complex; use a step limit instead
        let max_steps = max_cluster + 1;
        let mut steps: u32 = 0;

        while current >= 2 && current <= max_cluster && steps < max_steps {
            let idx = (current - 2) as usize;
            if idx >= used_by.len() {
                break;
            }

            if used_by[idx] != 0 && used_by[idx] != owner_id {
                if *cross_links < 5 {
                    out.push(b"\n  Cross-link: cluster ");
                    out.push_u32(current, 0);
                    out.push(b" used by entries ");
                    out.push_u32(used_by[idx] as u32, 0);
                    out.push(b" and ");
                    out.push_u32(owner_id as u32, 0);
                }
                *cross_links += 1;
                *errors += 1;
                break;
            }

            used_by[idx] = owner_id;
            count += 1;

            let next = get_fat_entry(fat, current);
            if is_eof(next) || is_free(next) || is_bad(next) {
                break;
            }
            current = next as u32;
            steps += 1;
        }

        if steps >= max_steps {
            out.push(b"\n  Loop detected in chain starting at cluster ");
            out.push_u32(start as u32, 0);
            *errors += 1;
        }

        count
    }

    // -----------------------------------------------------------------------
    // 4. Directory entry scan
    // -----------------------------------------------------------------------
    out.push(b"Scanning root directory...\n");

    let root_buf_ptr = alloc_buf(root_dir_size as usize);
    let root_buf = unsafe { core::slice::from_raw_parts_mut(root_buf_ptr, root_dir_size as usize) };
    disk.read_bytes(root_buf, root_dir_start);

    let mut owner_counter: u16 = 1;
    let mut file_count: u32 = 0;

    let entry_count = root_dir_entries as usize;
    for i in 0..entry_count {
        let offset = i * 32;
        if offset + 32 > root_buf.len() {
            break;
        }

        let entry: DirEntry = unsafe {
            core::ptr::read_unaligned(root_buf.as_ptr().add(offset) as *const DirEntry)
        };

        // End of directory
        if entry.file_name[0] == 0x00 {
            break;
        }
        // Deleted entry
        if entry.file_name[0] == 0xE5 {
            continue;
        }
        // Volume label or long name
        if entry.attributes == DIR_ATTR_LONG_NAME || (entry.attributes & DIR_ATTR_VOLUME_LABEL) != 0 {
            continue;
        }

        let first_cluster = entry.first_file_cluster;
        let byte_size = entry.byte_size;
        let is_dir = (entry.attributes & DIR_ATTR_DIRECTORY) != 0;

        // Print entry info
        out.push(b"  ");
        // Print filename (trim trailing spaces)
        let mut name_len = 8;
        while name_len > 0 && entry.file_name[name_len - 1] == b' ' {
            name_len -= 1;
        }
        for j in 0..name_len {
            out.push_byte(entry.file_name[j]);
        }

        // Print extension
        let mut ext_len = 3;
        while ext_len > 0 && entry.ext[ext_len - 1] == b' ' {
            ext_len -= 1;
        }
        if ext_len > 0 {
            out.push_byte(b'.');
            for j in 0..ext_len {
                out.push_byte(entry.ext[j]);
            }
        }

        // Pad to 14 chars
        let printed = name_len + if ext_len > 0 { 1 + ext_len } else { 0 };
        for _ in printed..14 {
            out.push_byte(b' ');
        }

        if is_dir {
            out.push(b"<DIR>  ");
        } else {
            out.push_u32(byte_size, 6);
            out.push(b" bytes, ");
        }

        // Follow chain
        let owner_id = owner_counter;
        owner_counter = owner_counter.wrapping_add(1);
        if owner_counter == 0 { owner_counter = 1; }

        let chain_len = if first_cluster >= 2 {
            follow_chain(fat1, used_by, first_cluster, owner_id, max_cluster, &mut errors, &mut cross_links, &mut out)
        } else {
            0
        };

        out.push_u32(chain_len, 0);
        if chain_len == 1 {
            out.push(b" cluster");
        } else {
            out.push(b" clusters");
        }

        // Verify size vs chain length (files only)
        if !is_dir && first_cluster >= 2 {
            let expected_clusters = if byte_size == 0 {
                0u32
            } else {
                (byte_size + bytes_per_cluster - 1) / bytes_per_cluster
            };
            if chain_len != expected_clusters {
                out.push(b"  SIZE MISMATCH (expected ");
                out.push_u32(expected_clusters, 0);
                out.push(b")");
                errors += 1;
            } else {
                out.push(b"  OK");
            }
        } else if !is_dir && first_cluster < 2 && byte_size == 0 {
            out.push(b"  OK");
        } else {
            out.push(b"  OK");
        }
        out.push(b"\n");
        file_count += 1;
    }

    if file_count == 0 {
        out.push(b"  (no files)\n");
    }

    // -----------------------------------------------------------------------
    // 5. Lost cluster detection
    // -----------------------------------------------------------------------
    out.push(b"Checking for lost clusters...");

    let mut used_count: u32 = 0;
    let mut free_count: u32 = 0;
    let mut lost_count: u32 = 0;

    for c in 2..=max_cluster {
        let val = get_fat_entry(fat1, c);
        let idx = (c - 2) as usize;
        if is_free(val) {
            free_count += 1;
        } else {
            used_count += 1;
            if idx < used_by.len() && used_by[idx] == 0 {
                lost_count += 1;
            }
        }
    }

    if lost_count > 0 {
        out.push(b" FOUND ");
        out.push_u32(lost_count, 0);
        out.push(b" lost clusters\n");
        errors += lost_count;
    } else {
        out.push(b" OK\n");
    }

    // -----------------------------------------------------------------------
    // 6. Summary
    // -----------------------------------------------------------------------
    out.push(b"\n");
    out.push(b"  "); out.push_u32(total_sectors as u32, 5); out.push(b" total sectors\n");
    out.push(b"  "); out.push_u32(total_clusters, 5); out.push(b" data clusters\n");
    out.push(b"  "); out.push_u32(used_count, 5); out.push(b" used clusters\n");
    out.push(b"  "); out.push_u32(free_count, 5); out.push(b" free clusters\n");
    out.push(b"  "); out.push_u32(lost_count, 5); out.push(b" lost clusters\n");
    out.push(b"  "); out.push_u32(errors, 5); out.push(b" errors found\n");

    out.flush();
    let _ = close_sync(handle);
}
