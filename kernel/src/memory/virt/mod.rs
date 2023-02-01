//! # Virtual Memory and Paging #
//! The kernel's ability to isolate tasks, as well as simulate a full PC memory
//! space for each DOS task, comes from virtual memory. Every task has access
//! to a unique 3 GiB address space, beneath a shared memory area reserved for
//! the kernel.
//!
//! | Userspace ...                    | Kernel code and data |
//! 0                                  0xc0000000             0xffffffff
//!
//! The upper kernel space starts at 0xc0000000 and begins with the kernel
//! executable. After the code and data sections allocated by the ELF header,
//! as well as the space that was reserved for the Physical Memory frame map,
//! the kernel heap begins. The heap is used for alloc data types, and grows
//! upwards as it runs out of space.
//! At the very top of memory is a reference from the page directory to itself.
//! This is a convenient way to always make the current directory editable, at
//! the cost of only a single page of virtual memory space. Beneath that are a
//! few scratch pages -- 4KiB areas that can be temporarily mapped to any frame
//! of physical memory. This is used by the kernel to edit memory that may not
//! in the space of the current task.
//! Beneath the scratch space are the kernel stacks. Each task has its own
//! unique kernel stack with a fixed size. These are allocated downwards, with
//! the initial kernel setup/idle task taking the topmost stack. When a task
//! enters the kernel (by syscall, interrupt, or exception), this stack
//! used. When a task is terminated and cleaned up, its kernel stack is marked
//! as available for re-use, and will be allocated to the next task created.
//!
//! Kernel Space
//! | .text + .rodata | .data + .bss | heap ->  <- stacks | scratch | pagedir |
//! 0xc0000000                                                       0xffffffff

pub mod page_entry;
pub mod page_table;

use page_table::{PageTable, PageTableReference};
use super::address::VirtualAddress;
use super::physical::allocate_frame;

/// Create the initial page directory need to enable paging.
pub fn create_initial_pagedir() -> PageTableReference {
    let dir_address = allocate_frame().unwrap().to_physical_address();
    unsafe {
        // zero out the directory frame, otherwise it can cause strange bugs
        let frame_start = dir_address.as_u32() as *mut u8;
        let frame_slice = core::slice::from_raw_parts_mut(frame_start, super::physical::FRAME_SIZE);
        for i in 0..frame_slice.len() {
            frame_slice[i] = 0;
        }
    }

    let dir = PageTable::at_address(VirtualAddress::new(dir_address.into()));
    // Point the last entry to itself, so that it is always accessible
    dir.get_mut(1023).set_address(dir_address);
    dir.get_mut(1023).set_present();

    PageTableReference::new(dir_address)
}

/// Modify CPU registers to enable paging
pub fn enable_paging() {
    
}
