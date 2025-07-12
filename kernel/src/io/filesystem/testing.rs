use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU32, Ordering};
use idos_api::io::error::IOError;
use spin::RwLock;

use crate::files::path::Path;

use crate::io::async_io::{ASYNC_OP_READ, OPERATION_FLAG_MESSAGE};
use crate::io::driver::async_driver::AsyncDriver;
use crate::io::driver::comms::IOResult;
use crate::io::driver::kernel_driver::KernelDriver;
use crate::io::handle::PendingHandleOp;
use crate::task::actions::handle::open_message_queue;
use crate::task::actions::send_message;
use crate::task::id::TaskID;
use crate::task::messaging::Message;

pub mod sync_fs {
    use crate::io::filesystem::driver::AsyncIOCallback;

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
            Self { written: 0 }
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

    impl KernelDriver for TestSyncFS {
        fn open(&self, path: Option<Path>, _: AsyncIOCallback) -> Option<IOResult> {
            let result = match path {
                Some(path) => {
                    if path.as_str() == "MYFILE.TXT" {
                        let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
                        self.open_files.write().insert(instance, OpenFile::new());
                        Ok(instance)
                    } else {
                        Err(IOError::NotFound)
                    }
                }
                None => Err(IOError::NotFound),
            };
            Some(result)
        }

        fn read(
            &self,
            instance: u32,
            buffer: &mut [u8],
            _offset: u32,
            _: AsyncIOCallback,
        ) -> Option<IOResult> {
            let mut open_files = self.open_files.write();
            let found = match open_files.get_mut(&instance) {
                Some(file) => file,
                None => return Some(Err(IOError::FileHandleInvalid)),
            };
            for i in 0..buffer.len() {
                let value = ((found.written + i) % 26) + 0x41;
                buffer[i] = value as u8;
            }
            found.written += buffer.len();
            Some(Ok(buffer.len() as u32))
        }

        fn close(&self, _instance: u32, _io_callback: AsyncIOCallback) -> Option<IOResult> {
            panic!("not implemented");
        }
    }
}

pub mod async_fs {
    use idos_api::io::AsyncOp;

    use crate::task::actions::{
        io::{driver_io_complete, send_io_op},
        sync::{block_on_wake_set, create_wake_set},
    };

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
            Self { written: 0 }
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

        fn read(&mut self, instance: u32, buffer: &mut [u8], _offset: u32) -> IOResult {
            let mut open_files = self.open_files.write();
            let found = open_files
                .get_mut(&instance)
                .ok_or(IOError::FileHandleInvalid)?;
            for i in 0..buffer.len() {
                let value = ((found.written + i) % 26) + 0x41;
                buffer[i] = value as u8;
            }
            found.written += buffer.len();
            Ok(buffer.len() as u32)
        }

        fn close(&mut self, _instance: u32) -> IOResult {
            panic!("not implemented");
        }
    }

    pub fn driver_task() -> ! {
        let message_handle = open_message_queue();
        let mut message = Message::empty();
        let message_ptr = &mut message as *mut Message as u32;

        let mut driver_impl = AsyncTestFS::new();

        let wake_set = create_wake_set();

        let mut op = AsyncOp::new(
            ASYNC_OP_READ,
            message_ptr,
            core::mem::size_of::<Message>() as u32,
            0,
        );
        let _ = send_io_op(message_handle, &op, Some(wake_set));

        loop {
            if op.is_complete() {
                op = AsyncOp::new(
                    ASYNC_OP_READ,
                    message_ptr,
                    core::mem::size_of::<Message>() as u32,
                    0,
                );
                let _ = send_io_op(message_handle, &op, Some(wake_set));
            } else {
                // Wait for the next message
                block_on_wake_set(wake_set, None);
                continue;
            }
            let request_id = message.unique_id;
            match driver_impl.handle_request(message) {
                Some(response) => driver_io_complete(request_id, response),
                None => continue,
            }
        }
    }
}

pub mod async_dev {
    use idos_api::io::AsyncOp;

    use crate::task::actions::{
        io::{driver_io_complete, send_io_op},
        sync::{block_on_wake_set, create_wake_set},
    };

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
            Self { written: 0 }
        }
    }

    impl AsyncDriver for AsyncTestDev {
        fn open(&mut self, _path: &str) -> IOResult {
            let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
            self.open_files.write().insert(instance, OpenFile::new());
            Ok(instance)
        }

        fn read(&mut self, instance: u32, buffer: &mut [u8], _offset: u32) -> IOResult {
            let mut open_files = self.open_files.write();
            let found = open_files
                .get_mut(&instance)
                .ok_or(IOError::FileHandleInvalid)?;
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

        fn close(&mut self, _instance: u32) -> IOResult {
            panic!("not implemented");
        }
    }

    pub fn driver_task() -> ! {
        let message_handle = open_message_queue();
        let mut message = Message::empty();
        let message_ptr = &mut message as *mut Message as u32;

        let mut driver_impl = AsyncTestDev::new();
        let wake_set = create_wake_set();

        let mut op = AsyncOp::new(
            ASYNC_OP_READ,
            message_ptr,
            core::mem::size_of::<Message>() as u32,
            0,
        );
        let _ = send_io_op(message_handle, &op, Some(wake_set));

        loop {
            if op.is_complete() {
                op = AsyncOp::new(
                    ASYNC_OP_READ,
                    message_ptr,
                    core::mem::size_of::<Message>() as u32,
                    0,
                );
                let _ = send_io_op(message_handle, &op, Some(wake_set));
            } else {
                // Wait for the next message
                block_on_wake_set(wake_set, None);
                continue;
            }
            let request_id = message.unique_id;
            match driver_impl.handle_request(message) {
                Some(response) => driver_io_complete(request_id, response),
                None => continue,
            }
        }
    }
}
