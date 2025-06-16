pub mod list_allocator;

use alloc::alloc::{GlobalAlloc, Layout};
use list_allocator::ListAllocator;
use spin::Mutex;

use crate::memory::physical::allocate_frame;
use crate::task::paging::{current_pagedir_map, get_current_physical_address, PermissionFlags};

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
        loop {
            let ptr = allocator.alloc(layout);
            if ptr.is_null() {
                let space_needed = layout.size();
                let pages_needed = (space_needed / 0x1000) + 1;
                allocator.expand(pages_needed);
            } else {
                return ptr;
            }
        }
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
    if get_current_physical_address(location).is_none() {
        let frame = allocate_frame().unwrap();
        current_pagedir_map(
            frame,
            location.prev_page_barrier(),
            PermissionFlags::empty(),
        );
    }
    // add at least one more page
    let extra_page = location + 0x1000;
    let extra_frame = allocate_frame().unwrap();
    current_pagedir_map(
        extra_frame,
        extra_page.prev_page_barrier(),
        PermissionFlags::empty(),
    );
    let heap_end = (location + 0x1000).next_page_barrier();
    let byte_size = heap_end.as_u32() - location.as_u32();

    ALLOCATOR.update_implementation(location.as_u32() as usize, byte_size as usize);

    super::LOGGER.log(format_args!(
        "Kernel Heap at {:?}, {:#X} bytes",
        location, byte_size
    ));
}

#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
    panic!("Alloc error: {:?}", layout);
}
