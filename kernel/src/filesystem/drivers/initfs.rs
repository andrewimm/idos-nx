use crate::collections::SlotList;
use crate::files::handle::DriverHandle;
use crate::files::path::Path;
use spin::RwLock;
use super::super::kernel::KernelFileSystem;


/// The Init FS is an in-memory, read-only system containing the files needed
/// to boot the system.
pub struct InitFileSystem {
    open_handle_map: RwLock<SlotList<OpenHandle>>,
}

impl InitFileSystem {
    pub const fn new() -> Self {
        Self {
            open_handle_map: RwLock::new(SlotList::new()),
        }
    }

    fn get_open_file(&self, handle: DriverHandle) -> Option<OpenFile> {
        let index: usize = handle.into();
        self.open_handle_map.read()
            .get(index)
            .and_then(|handle| {
                match handle {
                    OpenHandle::File(f) => Some(f.clone()),
                }
            })
    }
}

impl KernelFileSystem for InitFileSystem {
    fn open(&self, path: Path) -> Result<DriverHandle, ()> {
        let open_file = OpenFile {};
        let handle = OpenHandle::File(open_file);
        let index = self.open_handle_map.write().insert(handle);
        crate::kprint!("INITFS open file: {}\n", path.as_str());
        Ok(DriverHandle(index as u32))
    }

    fn read(&self, handle: DriverHandle, buffer: &mut [u8]) -> Result<usize, ()> {
        let open_file = self.get_open_file(handle).ok_or(())?;
        for i in 0..buffer.len() {
            buffer[i] = b'A';
        }
        Ok(buffer.len())
    }

    fn write(&self, handle: DriverHandle, buffer: &[u8]) -> Result<usize, ()> {
        Err(())
    }

    fn close(&self, handle: DriverHandle) -> Result<(), ()> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct OpenFile {
}

pub enum OpenHandle {
    File(OpenFile),
}

impl OpenHandle {
    pub fn is_file(&self) -> bool {
        match self {
            OpenHandle::File(_) => true,
        }
    }
}

