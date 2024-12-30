use super::bpb::BiosParamBlock;
use super::dir::RootDirectory;
use super::disk::DiskAccess;
use super::table::AllocationTable;

pub struct FatFS {
    pub disk: DiskAccess,
    pub bpb: BiosParamBlock,
    pub table: AllocationTable,
}

impl FatFS {
    pub fn new(mount: &str) -> Self {
        let mut disk = DiskAccess::new(mount);

        let mut volume_label: [u8; 11] = [0x20; 11];
        disk.read_bytes_from_disk(0x2b, &mut volume_label);
        let label_str = core::str::from_utf8(&volume_label).unwrap();
        crate::kprint!("FAT VOLUME LABEL: \"{}\"\n", label_str);

        let mut bpb = BiosParamBlock::new();
        disk.read_struct_from_disk(0xb, &mut bpb);

        let total_sectors = bpb.total_sectors;
        crate::kprint!("total sectors: {:#X}\n", total_sectors);

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
