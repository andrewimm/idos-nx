use core::arch::asm;

#[repr(C, packed)]
pub struct FatMetadata {
    pub sectors_per_cluster: u16,
    pub root_dir_sector: u16,
    pub root_cluster_sector: u16,
    pub disk_number: u8,
}

/// If a BIOS supports INT 13h extensions, commands can be issued using this
/// packet structure
#[repr(C, packed)]
pub struct DiskAddressPacket {
    pub packet_size: u8,
    pub always_zero: u8,
    pub sectors_to_transfer: u16,
    pub transfer_buffer_offset: u16,
    pub transfer_buffer_segment: u16,
    pub lba_low: u32,
    pub lba_high: u32,
}

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

pub static mut FAT_DATA: FatMetadata = FatMetadata {
    sectors_per_cluster: 1,
    root_dir_sector: 19,
    root_cluster_sector: 33,
    disk_number: 0,
};

pub extern "C" fn init_fat(disk_number: u16) {
    unsafe {
        let fat_header: &'static FatHeader = &*(0x7c00 as *const FatHeader);

        FAT_DATA.disk_number = disk_number as u8;
        FAT_DATA.sectors_per_cluster = fat_header.sectors_per_cluster as u16;
        
        let mut sector_count: u16 = fat_header.reserved_sector_count;
        let mut table_count = fat_header.fat_count;
        while table_count > 0 {
            sector_count += fat_header.sectors_per_fat;
            table_count -= 1;
        }
        FAT_DATA.root_dir_sector = sector_count;
        // TODO: set root cluster
    }
}

pub extern "C" fn read_sectors(lba: u16, dest: u16, count: u16) {
    let packet = DiskAddressPacket {
        packet_size: 16,
        always_zero: 0,
        sectors_to_transfer: count,
        transfer_buffer_offset: dest,
        transfer_buffer_segment: 0,
        lba_low: lba as u32,
        lba_high: 0,
    };

    let packet_address: u16 = &packet as *const DiskAddressPacket as u16;

    unsafe {
        let disk_number = FAT_DATA.disk_number as u16;

        asm!(
            "push si",
            "mov si, ax",
            "mov ax, 0x4200",
            "int 0x13",
            "pop si",
            in("ax") packet_address,
            in("dx") disk_number,
        );
    }
}

