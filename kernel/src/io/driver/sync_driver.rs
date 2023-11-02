use crate::files::path::Path;

use super::comms::IOResult;

pub trait SyncDriver {
    #![allow(unused_variables)]

    fn open(&self, path: Path) -> IOResult;

    fn read(&self, instance: u32, buffer: &mut [u8]) -> IOResult;
}
