use alloc::boxed::Box;
use alloc::sync::Arc;
use crate::files::handle::DriverHandle;
use crate::files::path::Path;
use crate::task::id::TaskID;
use super::kernel::KernelFileSystem;

/// Determines how the kernel handles requests to an installed filesystem
#[derive(Clone)]
pub enum FileSystemType {
    /// A synchronous filesystem embedded into the kernel. This should only be
    /// used for requests that can be fulfilled immediately, like a RAM FS or
    /// a virtual filesystem that exposes kernel internals.
    KernelSync(Arc<Box<dyn KernelFileSystem + Send + Sync>>),

    /// A filesystem driver that runs in its own Task, with communication
    /// passing through the Arbiter. It references the task that messages will
    /// be sent to.
    Async(TaskID),
}

/// 
#[derive(Clone)]
pub struct FileSystemDriver {
    fs_type: FileSystemType,
}

impl FileSystemDriver {
    pub fn new_sync(driver: Box<dyn KernelFileSystem + Send + Sync>) -> Self {
        Self {
            fs_type: FileSystemType::KernelSync(Arc::new(driver)),
        }
    }

    pub fn open(&self, path: Path) -> Result<DriverHandle, ()> {
        match &self.fs_type {
            FileSystemType::KernelSync(fs) => {
                fs.open(path)
            },
            FileSystemType::Async(id) => {
                // send a request through the Arbiter
                Err(())
            },
        }
    }

    pub fn read(&self, handle: DriverHandle, buffer: &mut [u8]) -> Result<usize, ()> {
        match &self.fs_type {
            FileSystemType::KernelSync(fs) => {
                fs.read(handle, buffer)
            },
            FileSystemType::Async(id) => {
                Err(())
            },
        }
    }
}

