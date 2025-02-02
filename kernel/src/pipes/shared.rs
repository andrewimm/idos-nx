use core::sync::atomic::{AtomicUsize, Ordering};

#[repr(C)]
pub struct SharedPipe<const N: usize> {
    read_index: AtomicUsize,
    write_index: AtomicUsize,
    buffer: [u8; N],
}

impl<const N: usize> SharedPipe<N> {
    pub const fn new() -> Self {
        Self {
            read_index: AtomicUsize::new(0),
            write_index: AtomicUsize::new(0),
            buffer: [0; N],
        }
    }

    fn next_index(current: usize) -> usize {
        (current + 1) % N
    }

    pub fn write(&self, value: u8) -> bool {
        let write_index = self.write_index.load(Ordering::Relaxed);
        let next_index = Self::next_index(write_index);
        let read_index = self.read_index.load(Ordering::Acquire);
        if next_index == read_index {
            // if the buffer is full, fail to write
            return false;
        }
        unsafe {
            let data_ptr: *mut u8 = self.buffer.as_ptr().offset(write_index as isize) as *mut u8;
            core::ptr::write_volatile(data_ptr, value);
        }
        // Guarantees the write comes before the Reader fetches it
        self.write_index.store(next_index, Ordering::Release);
        true
    }

    pub fn read(&self) -> Option<u8> {
        let read_index = self.read_index.load(Ordering::Relaxed);
        let write_index = self.write_index.load(Ordering::Acquire);
        if read_index == write_index {
            // If there is nothing to read from the buffer, return None
            return None;
        }
        let value = unsafe {
            let data_ptr: *const u8 = self.buffer.as_ptr().offset(read_index as isize);
            core::ptr::read_volatile(data_ptr)
        };
        let next_index = Self::next_index(read_index);
        self.read_index.store(next_index, Ordering::Release);
        Some(value)
    }
}
