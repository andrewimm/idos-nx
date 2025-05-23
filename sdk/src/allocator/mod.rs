use core::sync::atomic::{AtomicPtr, Ordering};

use alloc::alloc::{GlobalAlloc, Layout};
use idos_api::syscall::memory::map_memory;

// Eventually the allocator implementation should be pluggable, but for now
// we'll hard-code a simple bump allocator

pub struct Allocator {
    heap_start: AtomicPtr<u8>,
    current_pointer: AtomicPtr<u8>,
    heap_end: AtomicPtr<u8>,
}

impl Allocator {
    pub const fn new() -> Self {
        Self {
            heap_start: AtomicPtr::new(0 as *mut u8),
            current_pointer: AtomicPtr::new(0 as *mut u8),
            heap_end: AtomicPtr::new(0 as *mut u8),
        }
    }

    pub fn init(&self) {
        let heap_start = map_memory(None, 0x1000, None).unwrap() as *mut u8;
        let heap_end = unsafe { heap_start.add(0x1000) };
        self.heap_start.store(heap_start, Ordering::Relaxed);
        self.current_pointer.store(heap_start, Ordering::Relaxed);
        self.heap_end.store(heap_end, Ordering::Relaxed);
    }

    pub fn reset(self) {
        let start = self.heap_start.load(Ordering::Relaxed);
        for i in 0..0x1000 {
            unsafe {
                core::ptr::write_volatile(start.add(i), 0);
            }
        }
        self.current_pointer.store(start, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        let mut current_pointer = self.current_pointer.load(Ordering::Relaxed);

        if current_pointer as usize % align != 0 {
            current_pointer = current_pointer.add(align - (current_pointer as usize % align));
        }

        if current_pointer.add(size) >= self.heap_end.load(Ordering::Relaxed) {
            return core::ptr::null_mut();
        }

        let result = current_pointer;
        self.current_pointer.fetch_ptr_add(size, Ordering::Relaxed);
        result
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // no-op
    }
}

#[global_allocator]
static ALLOC: Allocator = Allocator::new();

pub fn init_allocator() {
    ALLOC.init();
}

#[alloc_error_handler]
pub fn error_handler(_layout: Layout) -> ! {
    idos_api::syscall::exec::terminate(2);
}
