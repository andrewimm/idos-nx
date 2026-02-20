use super::{bpb::BiosParamBlock, disk::DiskAccess};

#[derive(Copy, Clone)]
pub struct AllocationTable {
    table_start: u32,
    sectors_per_cluster: u32,
    bytes_per_fat: u32,
    fat_count: u32,
    first_cluster_location: u32,
}

impl AllocationTable {
    pub fn from_bpb(bpb: BiosParamBlock) -> Self {
        let first_cluster_location =
            bpb.first_root_directory_sector() * 512 + bpb.root_directory_size();

        Self {
            table_start: bpb.reserved_sectors as u32 * 512,
            sectors_per_cluster: bpb.sectors_per_cluster as u32,
            bytes_per_fat: bpb.sectors_per_fat as u32 * 512,
            fat_count: bpb.fat_count as u32,
            first_cluster_location,
        }
    }

    pub fn bytes_per_cluster(&self) -> u32 {
        self.sectors_per_cluster * 512
    }

    pub fn get_cluster_location(&self, cluster: u32) -> u32 {
        self.first_cluster_location + self.bytes_per_cluster() * (cluster - 2)
    }

    pub fn get_next_cluster(&self, cluster: u32, disk: &mut DiskAccess) -> Option<u32> {
        let pair_offset = (cluster / 2) * 3;
        let mut pair_bytes: [u8; 3] = [0; 3];
        disk.read_bytes_from_disk(self.table_start + pair_offset, &mut pair_bytes);
        let pair_value =
            ((pair_bytes[2] as u32) << 16) | ((pair_bytes[1] as u32) << 8) | (pair_bytes[0] as u32);
        let next_cluster = if cluster & 1 == 0 {
            pair_value & 0xfff
        } else {
            (pair_value & 0xfff000) >> 12
        };
        if next_cluster == 0xfff {
            None
        } else {
            Some(next_cluster)
        }
    }

    pub fn get_nth_cluster(
        &self,
        first_cluster: u32,
        index: u32,
        disk: &mut DiskAccess,
    ) -> Option<u32> {
        let mut current = first_cluster;
        for _ in 0..index {
            current = self.get_next_cluster(current, disk)?;
        }
        Some(current)
    }

    /// Write a 12-bit value into the FAT entry for `cluster`.
    /// Updates all FAT copies.
    pub fn set_cluster_entry(&self, cluster: u32, value: u32, disk: &mut DiskAccess) {
        let value = value & 0xFFF;
        for fat_index in 0..self.fat_count {
            let fat_start = self.table_start + fat_index * self.bytes_per_fat;
            let pair_offset = (cluster / 2) * 3;
            let mut pair_bytes: [u8; 3] = [0; 3];
            disk.read_bytes_from_disk(fat_start + pair_offset, &mut pair_bytes);

            let mut pair_value = ((pair_bytes[2] as u32) << 16)
                | ((pair_bytes[1] as u32) << 8)
                | (pair_bytes[0] as u32);

            if cluster & 1 == 0 {
                // even cluster: low 12 bits
                pair_value = (pair_value & 0xFFF000) | value;
            } else {
                // odd cluster: high 12 bits
                pair_value = (pair_value & 0x000FFF) | (value << 12);
            }

            pair_bytes[0] = (pair_value & 0xFF) as u8;
            pair_bytes[1] = ((pair_value >> 8) & 0xFF) as u8;
            pair_bytes[2] = ((pair_value >> 16) & 0xFF) as u8;
            disk.write_bytes_to_disk(fat_start + pair_offset, &pair_bytes);
        }
    }

    /// Find the first free cluster (value == 0x000) starting from cluster 2,
    /// mark it as EOF (0xFFF), and return it.
    pub fn allocate_cluster(&self, disk: &mut DiskAccess) -> Option<u32> {
        // FAT12 max clusters is 4084
        for cluster in 2..4085u32 {
            let pair_offset = (cluster / 2) * 3;
            let mut pair_bytes: [u8; 3] = [0; 3];
            disk.read_bytes_from_disk(self.table_start + pair_offset, &mut pair_bytes);
            let pair_value = ((pair_bytes[2] as u32) << 16)
                | ((pair_bytes[1] as u32) << 8)
                | (pair_bytes[0] as u32);
            let entry = if cluster & 1 == 0 {
                pair_value & 0xFFF
            } else {
                (pair_value >> 12) & 0xFFF
            };
            if entry == 0x000 {
                self.set_cluster_entry(cluster, 0xFFF, disk);
                return Some(cluster);
            }
        }
        None
    }

    /// Free an entire cluster chain starting from `first_cluster`.
    pub fn free_chain(&self, first_cluster: u32, disk: &mut DiskAccess) {
        let mut current = first_cluster;
        loop {
            let next = self.get_next_cluster(current, disk);
            self.set_cluster_entry(current, 0x000, disk);
            match next {
                Some(n) => current = n,
                None => break,
            }
        }
    }
}
