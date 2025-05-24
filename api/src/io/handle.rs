#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Handle(u32);

impl Handle {
    pub fn new(handle: u32) -> Self {
        Handle(handle)
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

pub fn dup_handle(handle: Handle) -> Option<Handle> {
    let new_handle = crate::syscall::syscall(0x2b, handle.as_u32(), 0, 0);
    match new_handle {
        0xffff_ffff => None,
        _ => Some(Handle::new(new_handle)),
    }
}

pub fn transfer_handle(handle: Handle, task_id: u32) -> Option<Handle> {
    let new_handle = crate::syscall::syscall(0x2a, handle.as_u32(), task_id, 0);
    match new_handle {
        0xffff_ffff => None,
        _ => Some(Handle::new(new_handle)),
    }
}
