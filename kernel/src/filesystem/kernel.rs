use crate::files::handle::DriverHandle;
use crate::files::path::Path;

pub trait KernelFileSystem {
    #![allow(unused_variables)]

    fn open(&self, path: Path) -> Result<DriverHandle, ()>;

    fn read(&self, handle: DriverHandle, buffer: &mut [u8]) -> Result<usize, ()>;

    fn write(&self, handle: DriverHandle, buffer: &[u8]) -> Result<usize, ()>;

    fn close(&self, handle: DriverHandle) -> Result<(), ()>;

    fn configure(&self, command: u32, arg0: u32, arg1: u32, arg2: u32, arg3: u32) -> Result<u32, ()> {
        Err(())
    }
}
