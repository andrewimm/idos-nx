use alloc::alloc::{GlobalAlloc, Layout};

pub struct Allocator {
}

impl Allocator {
    pub const fn new() -> Self {
        Self {}
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
    }
}

#[global_allocator]
static ALLOC: Allocator = Allocator::new();

pub fn init_allocator() {
}

#[alloc_error_handler]
pub fn error_handler(_layout: Layout) -> ! {
    let stdout = idos_api::io::handle::FileHandle(1);
    idos_api::syscall::io::write_str(stdout, "Error allocating heap\n");
    idos_api::syscall::exec::terminate(1);
}
