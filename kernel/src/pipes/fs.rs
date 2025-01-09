use super::Pipe;
use crate::collections::SlotList;
use crate::files::handle::DriverHandle;
use spin::RwLock;

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
