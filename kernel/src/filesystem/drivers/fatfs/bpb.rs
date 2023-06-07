#[repr(C, packed)]
pub struct BiosParamBlock {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub fat_count: u8,
    pub root_directory_entries: u16,
    pub total_sectors: u16,
    pub media_descriptor: u8,
    pub sectors_per_fat: u16,
}

impl BiosParamBlock {
    pub fn new() -> Self {
        Self {
            bytes_per_sector: 0,
            sectors_per_cluster: 0,
            reserved_sectors: 0,
            fat_count: 0,
            root_directory_entries: 0,
            total_sectors: 0,
            media_descriptor: 0,
            sectors_per_fat: 0,
        }
    }

    pub fn first_root_directory_sector(&self) -> u32 {
        let fat_sectors = (self.fat_count as u32) * (self.sectors_per_fat as u32);

        (self.reserved_sectors as u32) + fat_sectors
    }
}
