use alloc::sync::Arc;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Pretty run-of-the-mill SPSC lockless ring buffer
pub struct RingBuffer<'buffer, T: Sized> {
    data: &'buffer [T],
    size: usize,

    write_index: AtomicUsize,
    read_index: AtomicUsize,
}

impl<'buffer, T: Sized + Copy> RingBuffer<'buffer, T> {
    pub fn for_buffer(data: &'buffer [T]) -> Self {
        Self {
            data,
            size: data.len() - 1,

            write_index: AtomicUsize::new(0),
            read_index: AtomicUsize::new(0),
        }
    }

    pub fn split(self) -> (Reader<'buffer, T>, Writer<'buffer, T>) {
        let wrapped = Arc::new(self);
        let reader = Reader::new(wrapped.clone());
        let writer = Writer::new(wrapped);

        (reader, writer)
    }

    pub fn next_index(&self, current: usize) -> usize {
        (current + 1) % self.data.len()
    }

    pub fn write(&self, value: T) -> bool {
        // Only the Writer calls this method, so this can be relaxed
        let write_index = self.write_index.load(Ordering::Relaxed);
        let next_index = self.next_index(write_index);
        // This index is guaranteed to come after the Reader updates it
        let read_index = self.read_index.load(Ordering::Acquire);
        if next_index == read_index {
            // full
            return false;
        }
        unsafe {
            let data_ptr: *mut T = self.data.as_ptr() as *mut T;
            let dest = data_ptr.offset(write_index as isize);
            *dest = value;
        }
        // Guarantees the write comes before the Reader fetches it
        self.write_index.store(next_index, Ordering::Release);
        true
    }

    pub fn read(&self) -> Option<T> {
        let read_index = self.read_index.load(Ordering::Relaxed);
        let write_index = self.write_index.load(Ordering::Acquire);
        if read_index == write_index {
            // empty
            return None;
        }
        let value = unsafe {
            let data_ptr: *const T = self.data.as_ptr();
            let src = data_ptr.offset(read_index as isize);
            *src
        };
        let next_index = self.next_index(read_index);
        self.read_index.store(next_index, Ordering::Release);
        return Some(value);
    }

    pub fn capacity(&self) -> usize {
        self.size
    }
}

pub struct Reader<'buffer, T: Sized + Copy> {
    buffer: Arc<RingBuffer<'buffer, T>>,
}

impl<'buffer, T: Sized + Copy> Reader<'buffer, T> {
    pub fn new(buffer: Arc<RingBuffer<'buffer, T>>) -> Self {
        Self {
            buffer,
        }
    }

    pub fn read(&self) -> Option<T> {
        self.buffer.read()
    }
}

pub struct Writer<'buffer, T: Sized + Copy> {
    buffer: Arc<RingBuffer<'buffer, T>>,
}

impl<'buffer, T: Sized + Copy> Writer<'buffer, T> {
    pub fn new(buffer: Arc<RingBuffer<'buffer, T>>) -> Self {
        Self {
            buffer,
        }
    }

    pub fn write(&self, value: T) -> bool {
        self.buffer.write(value)
    }
}

