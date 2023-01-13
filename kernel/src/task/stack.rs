use alloc::boxed::Box;
use core::sync::atomic::AtomicU32;

extern {
    #[link_name = "__stack_start"]
    static mut label_stack_start: u8;
    #[link_name = "__stack_end"]
    static mut label_stack_end: u8;
}

/// This is super hacky, but it'll get better when we have paging
static NEXT_KERNEL_STACK: AtomicU32 = AtomicU32::new(0);

pub fn create_initial_stack() -> Box<[u8]> {
    // delete this when we have paging
    NEXT_KERNEL_STACK.store(unsafe { &label_stack_end as *const u8 as u32 }, core::sync::atomic::Ordering::SeqCst);    

    unsafe {
        let stack_start_ptr = &mut label_stack_start as *mut u8;
        alloc::vec::Vec::from_raw_parts(stack_start_ptr, 0x1000, 0x1000)
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
