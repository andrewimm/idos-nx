use alloc::boxed::Box;
use core::sync::atomic::AtomicU32;
use crate::memory::address::PhysicalAddress;
use crate::memory::virt::scratch::SCRATCH_BOTTOM;

extern {
    #[link_name = "__stack_start"]
    static mut label_stack_start: u8;
    #[link_name = "__stack_end"]
    static mut label_stack_end: u8;
}

/// Bottom of the scratch area, top of the kernel stacks
const KERNEL_STACKS_TOP: usize = SCRATCH_BOTTOM;
pub const STACK_SIZE_IN_PAGES: usize = 1;
pub const STACK_SIZE_IN_BYTES: usize = STACK_SIZE_IN_PAGES * 0x1000;

/// Return the physical location and size (in pages) of the initial kernel stack
pub fn get_initial_kernel_stack_location() -> (PhysicalAddress, usize) {
    let addr = PhysicalAddress::new(
        unsafe { &label_stack_start as *const u8 as u32 }
    );
    (addr, STACK_SIZE_IN_PAGES)
}

/// Return the distance between the virtual initial kernel stack and its
/// physical location.
pub fn get_kernel_stack_virtual_offset() -> usize {
    let physical_start = unsafe { &label_stack_start as *const u8 as usize };
    let virtual_start = KERNEL_STACKS_TOP - STACK_SIZE_IN_BYTES;
    virtual_start - physical_start
}

/// This is super hacky, but it'll get better when we have paging
static NEXT_KERNEL_STACK: AtomicU32 = AtomicU32::new(0);

pub fn create_initial_stack() -> Box<[u8]> {
    // delete this when we have paging
    NEXT_KERNEL_STACK.store(unsafe { &label_stack_end as *const u8 as u32 }, core::sync::atomic::Ordering::SeqCst);    

    let initial_stack_bottom = KERNEL_STACKS_TOP - STACK_SIZE_IN_BYTES;
    let initial_stack_ptr = initial_stack_bottom as *mut u8;
    unsafe {
        alloc::vec::Vec::from_raw_parts(initial_stack_ptr, STACK_SIZE_IN_BYTES, STACK_SIZE_IN_BYTES)
            .into_boxed_slice()
    }
}

pub fn free_stack(stack: Box<[u8]>) {
    let box_ptr = Box::into_raw(stack);
    // TODO: mark the stack as free and available for a new task
}

pub fn allocate_stack() -> Box<[u8]> {
    let stack_start = NEXT_KERNEL_STACK.fetch_add(0x1000, core::sync::atomic::Ordering::SeqCst);
    unsafe {
        alloc::vec::Vec::from_raw_parts(stack_start as *mut u8, 0x1000, 0x1000)
            .into_boxed_slice()
    }
}

