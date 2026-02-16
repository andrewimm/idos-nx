use core::arch::asm;

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
pub struct FatMetadata {
    pub sectors_per_cluster: u16,
    pub root_dir_sector: u16,
    pub root_cluster_sector: u16,
    pub disk_number: u8,
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
                self.next_entry as isize
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
            return Some((entry.first_cluster, entry.file_size_bytes));
        }
    }

    None
}

#[repr(C, packed)]
pub struct DiskAccessPacket {
    pub packet_size: u8,
    pub always_zero: u8,
    pub sectors_to_transfer: u16,
    pub transfer_buffer_offset: u16,
    pub transfer_buffer_segment: u16,
    pub lba_low: u32,
    pub lba_high: u32,
}

/// Load the FAT table from disk into memory at 0x5000 (segment 0x500).
/// Reads BPB fields from the boot sector still at 0x7C00.
pub fn load_fat_table(disk_number: u8) {
    let bpb = 0x7C00 as *const FatHeader;
    let reserved = unsafe { (*bpb).reserved_sector_count };
    let fat_sectors = unsafe { (*bpb).sectors_per_fat };
    // Load FAT to segment 0x500 (physical 0x5000), offset 0
    read_sectors(disk_number, reserved, 0x500, 0, fat_sectors);
}

/// Read a FAT12 entry for the given cluster from the FAT table at 0x5000.
/// Returns the next cluster number, or >= 0xFF8 for end-of-chain.
pub fn fat12_next(cluster: u16) -> u16 {
    let fat_base = 0x5000 as *const u8;
    let offset = (cluster as usize) * 3 / 2;
    let raw = unsafe {
        let lo = core::ptr::read_volatile(fat_base.add(offset)) as u16;
        let hi = core::ptr::read_volatile(fat_base.add(offset + 1)) as u16;
        lo | (hi << 8)
    };
    if cluster & 1 == 0 {
        raw & 0x0FFF
    } else {
        raw >> 4
    }
}

pub fn read_sectors(disk_number: u8, lba: u16, dest_segment: u16, dest_offset: u16, count: u16) {
    let packet = DiskAccessPacket {
        packet_size: 16,
        always_zero: 0,
        sectors_to_transfer: count,
        transfer_buffer_offset: dest_offset,
        transfer_buffer_segment: dest_segment,
        lba_low: lba as u32,
        lba_high: 0,
    };

    let packet_address: u16 = &packet as *const DiskAccessPacket as u16;

    unsafe {
        asm!(
            "push si",
            "mov si, ax",
            "mov ax, 0x4200",
            "int 0x13",
            "pop si",
            in("ax") packet_address,
            in("dx") disk_number as u16,
        );
    }
}
