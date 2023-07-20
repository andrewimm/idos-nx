use super::{bpb::BiosParamBlock, disk::DiskAccess};

#[derive(Copy, Clone)]
pub struct AllocationTable {
    table_start: u32,
    sectors_per_cluster: u32,
    sectors_per_table: u32,
    first_cluster_location: u32,
}

impl AllocationTable {
    pub fn from_bpb(bpb: BiosParamBlock) -> Self {
        let first_cluster_location = bpb.first_root_directory_sector() * 512 + bpb.root_directory_size();
        
        Self {
            table_start: bpb.reserved_sectors as u32 * 512,
            sectors_per_cluster: bpb.sectors_per_cluster as u32,
            sectors_per_table: bpb.sectors_per_fat as u32,
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
        let pair_value = ((pair_bytes[2] as u32) << 16)
            | ((pair_bytes[1] as u32) << 8)
            | (pair_bytes[0] as u32);
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

    pub fn get_nth_cluster(&self, first_cluster: u32, index: u32, disk: &mut DiskAccess) -> Option<u32> {
        let mut current = first_cluster;
        for _ in 0..index {
            current = self.get_next_cluster(current, disk)?;
        }
        Some(current)
    }
}

