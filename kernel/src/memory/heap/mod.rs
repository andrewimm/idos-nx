pub mod list_allocator;

use alloc::alloc::{GlobalAlloc, Layout};
use list_allocator::ListAllocator;
use spin::Mutex;

use super::address::VirtualAddress;

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
        let ptr = allocator.alloc(layout);
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

#[global_allocator]
static ALLOCATOR: Allocator = Allocator::new();

pub fn init_allocator(location: VirtualAddress) {
    // Initial heap size is from the start location to the end of the frame.
    // This memory should already have been allocated and mapped by previous
    // initialization tasks.
    // TODO: Until the heap is expandable, pre-allocating more memory. This
    // also needs to page memory
    let heap_end = location.next_page_barrier() + 0x1000;
    let byte_size = heap_end.as_u32() - location.as_u32();

    ALLOCATOR.update_implementation(location.as_u32() as usize, byte_size as usize);

    crate::kprint!("Kernel Heap at {:?}, {:#X} bytes\n", location, byte_size);
}

#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
    panic!("Alloc error: {:?}", layout);
}
