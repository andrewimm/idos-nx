use crate::bpb::BiosParamBlock;
use crate::dir::RootDirectory;
use crate::disk::{DiskAccess, DiskIO};
use crate::table::AllocationTable;

pub struct FatFS<D: DiskIO> {
    pub disk: DiskAccess<D>,
    pub bpb: BiosParamBlock,
    pub table: AllocationTable,
}

impl<D: DiskIO> FatFS<D> {
    pub fn new(disk_io: D) -> Self {
        let mut disk = DiskAccess::new(disk_io, 4096);

        let mut bpb = BiosParamBlock::new();
        disk.read_struct_from_disk(0xb, &mut bpb);

        let table = AllocationTable::from_bpb(bpb);

        Self { disk, bpb, table }
    }

    pub fn get_root_directory(&self) -> RootDirectory {
        RootDirectory::new(
            self.bpb.first_root_directory_sector(),
            self.bpb.root_directory_entries as u32,
        )
    }
}
