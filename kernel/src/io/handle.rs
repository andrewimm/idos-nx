use alloc::boxed::Box;
use idos_api::io::error::IOError;

use crate::{
    collections::SlotList,
    memory::{address::VirtualAddress, signal::Signal},
};

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
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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

    pub fn get_mut(&mut self, handle: Handle) -> Option<&mut T> {
        self.inner.get_mut(*handle)
    }

    pub fn remove(&mut self, handle: Handle) -> Option<T> {
        self.inner.remove(*handle)
    }

    pub fn replace(&mut self, handle: Handle, value: T) -> Option<T> {
        self.inner.replace(*handle, value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Handle, &T)> {
        self.inner
            .enumerate()
            .map(|(index, item)| (Handle::new(index), item))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Handle, &mut T)> {
        self.inner
            .enumerate_mut()
            .map(|(index, item)| (Handle::new(index), item))
    }
}

#[must_use]
pub struct PendingHandleOp {
    signal: Option<Signal>,
    return_value: Box<u32>,
}

impl PendingHandleOp {
    pub fn new(handle: Handle, op_code: u32, arg0: u32, arg1: u32, arg2: u32) -> Self {
        let signal = Signal::new();
        let return_value = Box::new(0);

        let return_value_ptr = return_value.as_ref() as *const u32;
        let op = AsyncOp::new(
            op_code,
            signal.get_address(),
            VirtualAddress::new(return_value_ptr as u32),
            arg0,
            arg1,
            arg2,
        );

        crate::task::actions::handle::add_io_op(handle, op).unwrap();

        Self {
            signal: Some(signal),
            return_value,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.signal.as_ref().unwrap().is_complete()
    }

    pub fn get_result(&self) -> Option<u32> {
        if self.is_complete() {
            return Some(*self.return_value);
        }
        None
    }

    pub fn wait_for_completion(&self) -> u32 {
        loop {
            if self.signal.as_ref().unwrap().is_complete() {
                return *self.return_value;
            }
            crate::task::actions::yield_coop();
        }
    }

    pub fn wait_for_result(&self) -> Result<u32, IOError> {
        let code = self.wait_for_completion();
        if code & 0x80000000 != 0 {
            let io_error = IOError::try_from(code & 0x7fffffff).unwrap_or(IOError::Unknown);
            Err(io_error)
        } else {
            Ok(code)
        }
    }
}

impl Drop for PendingHandleOp {
    fn drop(&mut self) {
        let signal = self.signal.take().unwrap();
        let _ = signal.get_value();
    }
}

impl core::fmt::Debug for PendingHandleOp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PendingHandleOp")
            .field("signal", self.signal.as_ref().unwrap())
            .field("return_value", &self.return_value)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{Handle, HandleTable};

    #[test_case]
    fn handle_table() {
        let mut table = HandleTable::<u32>::new();
        let a = table.insert(5);
        let b = table.insert(7);
        let c = table.insert(12);
        assert_eq!(table.get(a), Some(&5));
        assert_eq!(table.remove(b), Some(7));
        assert_eq!(table.get(b), None);
        let mut iter = table.iter();
        assert_eq!(iter.next(), Some((Handle::new(0), &5)));
        assert_eq!(iter.next(), Some((Handle::new(2), &12)));
        assert_eq!(iter.next(), None);
    }
}
