use core::sync::atomic::{AtomicU32, Ordering};
use alloc::collections::BTreeMap;
use idos_api::io::error::IOError;
use spin::RwLock;

use crate::files::path::Path;

use crate::io::driver::async_driver::AsyncDriver;
use crate::io::driver::comms::IOResult;
use crate::io::driver::sync_driver::SyncDriver;
use crate::task::actions::handle::open_message_queue;
use crate::task::actions::send_message;
use crate::task::messaging::Message;
use crate::task::id::TaskID;
use crate::io::handle::PendingHandleOp;
use crate::io::async_io::{OPERATION_FLAG_MESSAGE, MESSAGE_OP_READ};

pub mod sync_fs {
    use super::*;

    pub struct TestSyncFS {
        next_instance: AtomicU32,
        open_files: RwLock<BTreeMap<u32, OpenFile>>,
    }

    struct OpenFile {
        written: usize,
    }

    impl OpenFile {
        pub fn new() -> Self {
            Self {
                written: 0,
            }
        }
    }

    impl TestSyncFS {
        pub fn new() -> Self {
            Self {
                next_instance: AtomicU32::new(1),
                open_files: RwLock::new(BTreeMap::new()),
            }
        }
    }

    impl SyncDriver for TestSyncFS {
        fn open(&self, path: Path) -> IOResult {
            crate::kprintln!("TEST FS OPEN \"{}\"", path.as_str());
            if path.as_str() == "MYFILE.TXT" {
                let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
                self.open_files.write().insert(instance, OpenFile::new());
                Ok(instance)
            } else {
                Err(IOError::NotFound)
            }
        }

        fn read(&self, instance: u32, buffer: &mut [u8]) -> IOResult {
            let mut open_files = self.open_files.write();
            let found = open_files.get_mut(&instance).ok_or(IOError::FileHandleInvalid)?;
            for i in 0..buffer.len() {
                let value = ((found.written + i) % 26) + 0x41;
                buffer[i] = value as u8;
            }
            found.written += buffer.len();
            Ok(buffer.len() as u32)
        }
    }
}

pub mod async_fs {
    use super::*;

    pub struct AsyncTestFS {
        next_instance: AtomicU32,
        open_files: RwLock<BTreeMap<u32, OpenFile>>,
    }

    impl AsyncTestFS {
        pub fn new() -> Self {
            Self {
                next_instance: AtomicU32::new(1),
                open_files: RwLock::new(BTreeMap::new()),
            }
        }
    }

    struct OpenFile {
        written: usize,
    }

    impl OpenFile {
        pub fn new() -> Self {
            Self {
                written: 0,
            }
        }
    }

    impl AsyncDriver for AsyncTestFS {
        fn open(&mut self, path: &str) -> IOResult {
            crate::kprintln!("Async open \"{}\"", path);
            if path == "MYFILE.TXT" {
                let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
                self.open_files.write().insert(instance, OpenFile::new());
                Ok(instance)
            } else {
                Err(IOError::NotFound)
            }
        }

        fn read(&mut self, instance: u32, buffer: &mut [u8]) -> IOResult {
            let mut open_files = self.open_files.write();
            let found = open_files.get_mut(&instance).ok_or(IOError::FileHandleInvalid)?;
            for i in 0..buffer.len() {
                let value = ((found.written + i) % 26) + 0x41;
                buffer[i] = value as u8;
            }
            found.written += buffer.len();
            Ok(buffer.len() as u32)
        }
    }

    pub fn driver_task() -> ! {
        let message_handle = open_message_queue();
        let mut message = Message(0, 0, 0, 0);
        let message_ptr = &mut message as *mut Message as u32;

        let mut driver_impl = AsyncTestFS::new();

        loop {
            let op = PendingHandleOp::new(message_handle, OPERATION_FLAG_MESSAGE | MESSAGE_OP_READ, message_ptr, 0, 0);
            let sender = op.wait_for_completion();

            match driver_impl.handle_request(message) {
                Some(response) => {
                    send_message(TaskID::new(sender), response, 0xffffffff)
                },
                None => continue,
            }
        }
    }
}

pub mod async_dev {
    use super::*;

    pub struct AsyncTestDev {
        next_instance: AtomicU32,
        open_files: RwLock<BTreeMap<u32, OpenFile>>,
    }

    impl AsyncTestDev {
        pub fn new() -> Self {
            Self {
                next_instance: AtomicU32::new(1),
                open_files: RwLock::new(BTreeMap::new()),
            }
        }
    }

    struct OpenFile {
        written: usize,
    }

    impl OpenFile {
        pub fn new() -> Self {
            Self {
                written: 0,
            }
        }
    }

    impl AsyncDriver for AsyncTestDev {
        fn open(&mut self, path: &str) -> IOResult {
            let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
            self.open_files.write().insert(instance, OpenFile::new());
            Ok(instance)
        }

        fn read(&mut self, instance: u32, buffer: &mut [u8]) -> IOResult {
            let mut open_files = self.open_files.write();
            let found = open_files.get_mut(&instance).ok_or(IOError::FileHandleInvalid)?;
            let offset = found.written % 4;
            let sample = [b't', b'e', b's', b't'];
            let mut written = 0;
            while written < buffer.len() {
                let offset = found.written % 4;
                buffer[written] = sample[offset];
                written += 1;
                found.written += 1;
            }
            Ok(written as u32)
        }
    }

    pub fn driver_task() -> ! {
        let message_handle = open_message_queue();
        let mut message = Message(0, 0, 0, 0);
        let message_ptr = &mut message as *mut Message as u32;

        let mut driver_impl = AsyncTestDev::new();

        loop {
            let op = PendingHandleOp::new(message_handle, OPERATION_FLAG_MESSAGE | MESSAGE_OP_READ, message_ptr, 0, 0);
            let sender = op.wait_for_completion();

            match driver_impl.handle_request(message) {
                Some(response) => {
                    send_message(TaskID::new(sender), response, 0xffffffff)
                },
                None => continue,
            }
        }
    }
}

