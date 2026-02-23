//! Device Driver for PS/2 Mouse and Keyboard
//! This generic driver implements non-blocking, parallel read access to a
//! device that produces a byte string.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::{
    files::path::Path,
    io::{driver::kernel_driver::KernelDriver, filesystem::driver::AsyncIOCallback},
};
use alloc::collections::{BTreeMap, VecDeque};
use idos_api::io::error::{IoError, IoResult};
use spin::Mutex;

const MAX_BUFFER_SIZE: usize = 128;

static PORT_1_READERS: Mutex<BTreeMap<u32, OpenInstance>> = Mutex::new(BTreeMap::new());
static PORT_2_READERS: Mutex<BTreeMap<u32, OpenInstance>> = Mutex::new(BTreeMap::new());

pub fn push_port_1_data(data: &[u8]) {
    let mut readers = PORT_1_READERS.lock();
    for instance in readers.values_mut() {
        instance.buffered_data.extend(data);
        while instance.buffered_data.len() > MAX_BUFFER_SIZE {
            instance.buffered_data.pop_front();
        }
    }
}

pub fn push_port_2_data(data: &[u8]) {
    let mut readers = PORT_2_READERS.lock();
    for instance in readers.values_mut() {
        instance.buffered_data.extend(data);
        while instance.buffered_data.len() > MAX_BUFFER_SIZE {
            instance.buffered_data.pop_front();
        }
    }
}

enum Port {
    One,
    Two,
}

struct OpenInstance {
    buffered_data: VecDeque<u8>,
}

pub struct Ps2DeviceDriver {
    next_instance_id: AtomicU32,
    port: Port,
}

impl Ps2DeviceDriver {
    pub fn new(port: Port) -> Self {
        Self {
            next_instance_id: AtomicU32::new(1),
            port,
        }
    }
}

impl KernelDriver for Ps2DeviceDriver {
    fn open(
        &self,
        _path: Option<Path>,
        _flags: u32,
        _io_callback: AsyncIOCallback,
    ) -> Option<IoResult> {
        let instance_id = self.next_instance_id.fetch_add(1, Ordering::SeqCst);
        let open_instances = match self.port {
            Port::One => &PORT_1_READERS,
            Port::Two => &PORT_2_READERS,
        };
        open_instances.lock().insert(
            instance_id,
            OpenInstance {
                buffered_data: VecDeque::new(),
            },
        );
        Some(Ok(instance_id))
    }

    fn close(&self, instance: u32, _io_callback: AsyncIOCallback) -> Option<IoResult> {
        let open_instances = match self.port {
            Port::One => &PORT_1_READERS,
            Port::Two => &PORT_2_READERS,
        };
        let mut instances = open_instances.lock();
        if instances.remove(&instance).is_some() {
            Some(Ok(1))
        } else {
            Some(Err(IoError::FileHandleInvalid))
        }
    }

    fn read(
        &self,
        instance: u32,
        buffer: &mut [u8],
        _offset: u32,
        _io_callback: AsyncIOCallback,
    ) -> Option<IoResult> {
        let open_instances = match self.port {
            Port::One => &PORT_1_READERS,
            Port::Two => &PORT_2_READERS,
        };
        let mut instances = open_instances.lock();
        let instance_data = match instances.get_mut(&instance) {
            Some(data) => data,
            None => return Some(Err(IoError::FileHandleInvalid)),
        };

        let bytes_to_read = buffer.len().min(instance_data.buffered_data.len());
        for i in 0..bytes_to_read {
            buffer[i] = instance_data.buffered_data.pop_front().unwrap();
        }
        Some(Ok(bytes_to_read as u32))
    }
}

pub fn create_ps2_keyboard_driver() -> Ps2DeviceDriver {
    Ps2DeviceDriver::new(Port::One)
}

pub fn create_ps2_mouse_driver() -> Ps2DeviceDriver {
    Ps2DeviceDriver::new(Port::Two)
}
