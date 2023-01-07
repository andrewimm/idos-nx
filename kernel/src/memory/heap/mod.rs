pub mod list_allocator;

use alloc::alloc::{GlobalAlloc, Layout};
use list_allocator::ListAllocator;
use spin::Mutex;

struct Allocator {
    locked_allocator: Mutex<ListAllocator>,
}

impl Allocator {
    pub const fn new() -> Self {
        Self {
            locked_allocator: Mutex::new(ListAllocator::empty()),
        }
    }

    pub fn update_implementation(&self, location: usize, size: usize) {
        let mut allocator = self.locked_allocator.lock();
        *allocator = ListAllocator::new(location, size);
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.locked_allocator.lock();
        let mut ptr = allocator.alloc(layout);
        if ptr.is_null() {
            panic!("Heap expansion needs to be implemented");
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        let mut allocator = self.locked_allocator.lock();
        allocator.dealloc(ptr);
    }
}

pub const INITIAL_HEAP_SIZE: usize = 64;

#[global_allocator]
static ALLOCATOR: Allocator = Allocator::new();

pub fn init_allocator(location: usize, size: usize) {
    ALLOCATOR.update_implementation(location, size);
}

#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
    panic!("Alloc error: {:?}", layout);
}
