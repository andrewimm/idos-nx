use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU32, Ordering};
use idos_api::io::error::{IoError, IoResult};
use spin::RwLock;

use crate::files::path::Path;

use crate::io::async_io::{ASYNC_OP_READ, OPERATION_FLAG_MESSAGE};
use crate::io::driver::kernel_driver::KernelDriver;
use crate::io::handle::PendingHandleOp;
use crate::task::actions::handle::open_message_queue;
use crate::task::actions::send_message;
use crate::task::id::TaskID;
use idos_api::io::driver::AsyncDriver;
use idos_api::ipc::Message;

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
        fn open(&self, path: Option<Path>, _: AsyncIOCallback) -> Option<IoResult> {
            let result = match path {
                Some(path) => {
                    if path.as_str() == "MYFILE.TXT" {
                        let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
                        self.open_files.write().insert(instance, OpenFile::new());
                        Ok(instance)
                    } else {
                        Err(IoError::NotFound)
                    }
                }
                None => Err(IoError::NotFound),
            };
            Some(result)
        }

        fn read(
            &self,
            instance: u32,
            buffer: &mut [u8],
            _offset: u32,
            _: AsyncIOCallback,
        ) -> Option<IoResult> {
            let mut open_files = self.open_files.write();
            let found = match open_files.get_mut(&instance) {
                Some(file) => file,
                None => return Some(Err(IoError::FileHandleInvalid)),
            };
            for i in 0..buffer.len() {
                let value = ((found.written + i) % 26) + 0x41;
                buffer[i] = value as u8;
            }
            found.written += buffer.len();
            Some(Ok(buffer.len() as u32))
        }

        fn close(&self, _instance: u32, _io_callback: AsyncIOCallback) -> Option<IoResult> {
            panic!("not implemented");
        }

        fn share(
            &self,
            instance: u32,
            target_task_id: TaskID,
            is_move: bool,
            io_callback: AsyncIOCallback,
        ) -> Option<IoResult> {
            Some(Ok(1))
        }
    }
}

pub mod async_fs {
    use idos_api::io::{
        driver::{DriverFileReference, DriverMappingToken},
        AsyncOp,
    };

    use crate::{
        memory::{address::PhysicalAddress, virt::scratch::UnmappedPage},
        task::actions::{
            io::{driver_io_complete, send_io_op},
            sync::{block_on_wake_set, create_wake_set},
        },
    };

    use super::*;

    pub struct AsyncTestFS {
        next_instance: AtomicU32,
        open_files: RwLock<BTreeMap<u32, OpenFile>>,
        next_mapping_token: AtomicU32,
        mapping_tokens: RwLock<BTreeMap<alloc::string::String, u32>>,
    }

    impl AsyncTestFS {
        pub fn new() -> Self {
            Self {
                next_instance: AtomicU32::new(1),
                open_files: RwLock::new(BTreeMap::new()),
                next_mapping_token: AtomicU32::new(0xA0),
                mapping_tokens: RwLock::new(BTreeMap::new()),
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
        fn release_buffer(&mut self, buffer_ptr: *mut u8, buffer_len: usize) {
            use crate::memory::{address::VirtualAddress, shared::release_buffer};
            release_buffer(VirtualAddress::new(buffer_ptr as u32), buffer_len);
        }

        fn open(&mut self, path: &str) -> IoResult<DriverFileReference> {
            crate::kprintln!("Async open \"{}\"", path);
            if path == "MYFILE.TXT" {
                let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
                self.open_files.write().insert(instance, OpenFile::new());
                Ok(DriverFileReference::new(instance))
            } else {
                Err(IoError::NotFound)
            }
        }

        fn read(
            &mut self,
            file_ref: DriverFileReference,
            buffer: &mut [u8],
            _offset: u32,
        ) -> IoResult {
            let mut open_files = self.open_files.write();
            let found = open_files
                .get_mut(&*file_ref)
                .ok_or(IoError::FileHandleInvalid)?;
            for i in 0..buffer.len() {
                let value = ((found.written + i) % 26) + 0x41;
                buffer[i] = value as u8;
            }
            found.written += buffer.len();
            Ok(buffer.len() as u32)
        }

        fn create_mapping(&mut self, path: &str) -> IoResult<DriverMappingToken> {
            let mut tokens = self.mapping_tokens.write();
            let token = tokens
                .entry(alloc::string::String::from(path))
                .or_insert_with(|| self.next_mapping_token.fetch_add(1, Ordering::SeqCst));
            Ok(DriverMappingToken::new(*token))
        }

        fn remove_mapping(&mut self, _mapping: DriverMappingToken) -> IoResult {
            Ok(1)
        }

        fn page_in_mapping(
            &mut self,
            map_token: DriverMappingToken,
            offset: u32,
            frame_paddr: u32,
        ) -> IoResult {
            let tokens = self.mapping_tokens.read();
            if !tokens.values().any(|t| *t == *map_token) {
                return Err(IoError::InvalidArgument);
            }

            let frame_page = UnmappedPage::map(PhysicalAddress::new(frame_paddr));
            let frame_buffer_ptr = frame_page.virtual_address().as_ptr_mut::<u8>();
            let frame_buffer = unsafe { core::slice::from_raw_parts_mut(frame_buffer_ptr, 0x1000) };
            frame_buffer[0..9].copy_from_slice(b"PAGE DATA");
            frame_buffer[9..0x1000].fill(0xff);
            Ok(0x1000)
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
    use idos_api::io::{
        driver::{DriverFileReference, DriverMappingToken},
        AsyncOp,
    };

    use crate::{
        memory::{address::PhysicalAddress, virt::scratch::UnmappedPage},
        task::actions::{
            io::{driver_io_complete, send_io_op},
            sync::{block_on_wake_set, create_wake_set},
        },
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
        fn release_buffer(&mut self, buffer_ptr: *mut u8, buffer_len: usize) {
            use crate::memory::{address::VirtualAddress, shared::release_buffer};
            release_buffer(VirtualAddress::new(buffer_ptr as u32), buffer_len);
        }

        fn open(&mut self, _path: &str) -> IoResult<DriverFileReference> {
            let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
            self.open_files.write().insert(instance, OpenFile::new());
            Ok(DriverFileReference::new(instance))
        }

        fn read(
            &mut self,
            file_ref: DriverFileReference,
            buffer: &mut [u8],
            _offset: u32,
        ) -> IoResult {
            let mut open_files = self.open_files.write();
            let found = open_files
                .get_mut(&*file_ref)
                .ok_or(IoError::FileHandleInvalid)?;
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

        fn create_mapping(&mut self, path: &str) -> IoResult<DriverMappingToken> {
            Ok(DriverMappingToken::new(1))
        }

        fn remove_mapping(&mut self, _mapping: DriverMappingToken) -> IoResult {
            Ok(1)
        }

        fn page_in_mapping(
            &mut self,
            map_token: DriverMappingToken,
            offset_in_file: u32,
            frame_paddr: u32,
        ) -> IoResult {
            let frame_page = UnmappedPage::map(PhysicalAddress::new(frame_paddr));
            let page_buffer_ptr = frame_page.virtual_address().as_ptr_mut::<u8>();
            let page_buffer = unsafe { core::slice::from_raw_parts_mut(page_buffer_ptr, 0x1000) };

            let frame_start_offset = offset_in_file & 0xfff;
            let first_fill_length = (0x1000 - frame_start_offset) as usize;
            let fill_value = b'A' + ((offset_in_file / 0x1000) % 26) as u8;
            let second_fill_value = fill_value + 1;
            for i in 0..first_fill_length {
                page_buffer[i] = fill_value;
            }
            for i in first_fill_length..0x1000 {
                page_buffer[i] = second_fill_value;
            }

            Ok(0x1000)
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
