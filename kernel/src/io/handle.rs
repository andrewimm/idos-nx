use core::sync::atomic::{AtomicU32, Ordering};

use alloc::boxed::Box;

use crate::{collections::SlotList, memory::address::VirtualAddress};

use super::async_io::AsyncOp;

/// A Handle represents a reference to an object that can be passed back and
/// forth across the syscall line. Internally, it's just a usize numeric value.
/// The value within the handle represents a real index in a table of open
/// objects. If the shape of the table is known, an arbitrary handle can be
/// easily constructed -- ie, a file handle with value 0 should point to the
/// stdin io object.
/// Handles are used for all async IO. Each task has a table of active IO
/// objects, and Handles are used to index each entry in this table. Userspace
/// code uses Handles to tell the task which IO object should be manipulated.
#[derive(Copy, Clone)]
pub struct Handle(usize);

impl Handle {
    pub fn new(index: usize) -> Self {
        Self(index)
    }
}

impl core::ops::Deref for Handle {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct HandleTable<T> {
    inner: SlotList<T>,
}

impl<T> HandleTable<T> {
    pub fn new() -> Self {
        Self {
            inner: SlotList::new(),
        }
    }

    pub fn insert(&mut self, value: T) -> Handle {
        let index = self.inner.insert(value);
        Handle::new(index)
    }

    pub fn get(&self, handle: Handle) -> Option<&T> {
        self.inner.get(*handle)
    }

    pub fn remove(&mut self, handle: Handle) -> Option<T> {
        self.inner.remove(*handle)
    }

    pub fn replace(&mut self, handle: Handle, value: T) -> Option<T> {
        self.inner.replace(*handle, value)
    }
}

pub struct PendingHandleOp {
    semaphore: Box<AtomicU32>,
    return_value: Box<u32>,
}

impl PendingHandleOp {
    pub fn new(handle: Handle, op_code: u32, arg0: u32, arg1: u32, arg2: u32) -> Self {
        let semaphore = Box::new(AtomicU32::new(0));
        let return_value = Box::new(0);

        let semaphore_ptr = semaphore.as_mut_ptr();
        let return_value_ptr = return_value.as_ref() as *const u32;
        let op = AsyncOp::new(
            op_code,
            VirtualAddress::new(semaphore_ptr as u32),
            VirtualAddress::new(return_value_ptr as u32),
            arg0,
            arg1,
            arg2,
        );

        crate::task::actions::handle::add_io_op(handle, op);
        
        Self {
            semaphore,
            return_value,
        }
    }

    pub fn is_complete(&self) -> bool {
        let semaphore = self.semaphore.load(Ordering::SeqCst);
        semaphore != 0
    }

    pub fn wait_for_completion(&self) -> u32 {
        loop {
            let semaphore = self.semaphore.load(Ordering::SeqCst);
            if semaphore != 0 {
                return *self.return_value;
            }
            crate::task::actions::yield_coop();
        }
    }
}

