use super::bpb::BiosParamBlock;

#[derive(Copy, Clone)]
pub struct AllocationTable {
    sectors_per_cluster: u32,
    sectors_per_table: u32,
    first_cluster_location: u32,
}

impl AllocationTable {
    pub fn from_bpb(bpb: BiosParamBlock) -> Self {
        let first_cluster_location = bpb.first_root_directory_sector() * 512 + bpb.root_directory_size();
        
        Self {
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
}

