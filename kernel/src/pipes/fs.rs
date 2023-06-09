use core::sync::atomic::{AtomicU32, Ordering};
use crate::collections::SlotList;
use crate::files::cursor::SeekMethod;
use crate::files::handle::DriverHandle;
use crate::files::path::Path;
use crate::filesystem::kernel::KernelFileSystem;
use crate::task::actions::lifecycle::wait_for_io;
use crate::task::switching::{get_current_id, get_task};
use spin::RwLock;
use super::Pipe;

/// Stores the actual pipes that have been created by the kernel
pub static PIPES: RwLock<SlotList<Pipe>> = RwLock::new(SlotList::new());
/// Stores the file handles for the read and write ends, which point to their
/// respective Pipe instance
pub static PIPE_HANDLES: RwLock<SlotList<PipeHandle>> = RwLock::new(SlotList::new());

pub enum PipeHandle {
    Reader(usize),
    Writer(usize),
}

pub fn create_pipe() -> (DriverHandle, DriverHandle) {
    let index = PIPES.write().insert(Pipe::new());
    let mut handles = PIPE_HANDLES.write();
    let reader = PipeHandle::Reader(index);
    let read_handle = DriverHandle(handles.insert(reader) as u32);
    let writer = PipeHandle::Writer(index);
    let write_handle = DriverHandle(handles.insert(writer) as u32);

    (read_handle, write_handle)
}

pub struct PipeDriver {
}

impl PipeDriver {
    pub fn new() -> Self {
        Self {}
    }

    fn write_pipe(pipe_index: usize, buffer: &[u8]) -> Result<usize, ()> {
        let (ring_buffer, blocked) = PIPES.read()
            .get(pipe_index)
            .ok_or(())
            .map(|pipe| (pipe.get_ring_buffer(), pipe.get_blocked_reader()))?;

        let mut index = 0;
        while index < buffer.len() {
            if !ring_buffer.write(buffer[index]) {
                break;
            }
            index += 1;
        }
        if index > 0 {
            let task_lock = blocked.and_then(|id| get_task(id));
            if let Some(lock) = task_lock {
                let mut task = lock.write();
                task.io_complete();
            }
        }
        Ok(index)
    }

    fn read_pipe(pipe_index: usize, buffer: &mut [u8]) -> Result<usize, ()> {
        let ring_buffer = {
            let mut pipes = PIPES.write();
            let pipe = pipes.get_mut(pipe_index).ok_or(())?;
            pipe.set_blocked_reader(get_current_id());
            pipe.get_ring_buffer()
        };
        let mut index = 0;
        while index < buffer.len() {
            match ring_buffer.read() {
                Some(value) => buffer[index] = value,
                None => wait_for_io(None),
            }
            index += 1;
        }
        {
            let mut pipes = PIPES.write();
            let pipe = pipes.get_mut(pipe_index).unwrap();
            pipe.clear_blocked_reader();
        }
        Ok(index)
    }
}

impl KernelFileSystem for PipeDriver {
    fn open(&self, path: Path) -> Result<DriverHandle, ()> {
        Err(())
    }

    fn read(&self, handle: DriverHandle, buffer: &mut [u8]) -> Result<usize, ()> {
        let pipe_index = {
            match PIPE_HANDLES.read().get(handle.into()).ok_or(())? {
                PipeHandle::Reader(index) => *index,
                _ => return Err(()),
            }
        };
        PipeDriver::read_pipe(pipe_index, buffer)
    }

    fn write(&self, handle: DriverHandle, buffer: &[u8]) -> Result<usize, ()> {
        let pipe_index = {
            match PIPE_HANDLES.read().get(handle.into()).ok_or(())? {
                PipeHandle::Writer(index) => *index,
                _ => return Err(()),
            }
        };
        PipeDriver::write_pipe(pipe_index, buffer)
    }

    fn close(&self, handle: DriverHandle) -> Result<(), ()> {
        Err(())
    }

    fn seek(&self, handle: DriverHandle, offset: SeekMethod) -> Result<usize, ()> {
        Err(())
    }
}

