use super::super::floppy::FloppyDiskController;
use super::super::ide::IdeController;

pub enum StorageController {
    IDE(IdeController),
    Floppy(FloppyDiskController),
}
