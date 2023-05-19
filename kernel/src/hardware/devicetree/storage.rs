use super::super::floppy::FloppyDiskController;

pub enum StorageController {
    Floppy(FloppyDiskController),
}
