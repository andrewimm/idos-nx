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
pub mod scratch;

use core::arch::asm;
use page_table::{PageTable, PageTableReference};
use crate::task::stack::get_initial_kernel_stack_location;

use self::scratch::SCRATCH_PAGE_COUNT;

use super::address::{PhysicalAddress, VirtualAddress};
use super::physical::allocate_frame;

/// Create the initial page directory need to enable paging.
pub fn create_initial_pagedir() -> PageTableReference {
    let dir_address = allocate_frame().unwrap().to_physical_address();
    zero_frame(dir_address);

    let dir = PageTable::at_address(VirtualAddress::new(dir_address.into()));
    // Point the last entry to itself, so that it is always accessible
    dir.get_mut(1023).set_address(dir_address);
    dir.get_mut(1023).set_present();

    // Identity-map the kernel
    {
        let table_zero_frame = allocate_frame().unwrap().to_physical_address();
        zero_frame(table_zero_frame);
        // TODO: Actually map kernel bounds
        // Right now this is just identity-mapping the lower 4MiB as a hack
        dir.get_mut(0).set_address(table_zero_frame);
        dir.get_mut(0).set_present();

        let table_zero = PageTable::at_address(VirtualAddress::new(table_zero_frame.into()));
        for index in 0..1024 {
            table_zero.get_mut(index).set_address(PhysicalAddress::new(0x1000 * index as u32));
            table_zero.get_mut(index).set_present();
        }
    }

    // Create a page table for the second-highest entry in the pagedir.
    // This will be used to store mappings to scratch space and kernel stacks.
    {
        let last_table_address = allocate_frame().unwrap().to_physical_address();
        zero_frame(last_table_address);
        dir.get_mut(1022).set_address(last_table_address);
        dir.get_mut(1022).set_present();

        let last_table = PageTable::at_address(VirtualAddress::new(last_table_address.into()));
        let kernel_stack_index = 1023 - SCRATCH_PAGE_COUNT;

        let (kernel_stack_address, kernel_stack_size) = get_initial_kernel_stack_location();
        for i in 0..kernel_stack_size {
            let index = kernel_stack_index - i;
            let stack_offset = (kernel_stack_size - i - 1) * 0x1000;
            let stack_frame = kernel_stack_address + stack_offset as u32;
            last_table.get_mut(index).set_address(stack_frame);
            last_table.get_mut(index).set_present();
        }
    }

    PageTableReference::new(dir_address)
}

/// Zero out an allocated frame, does not work once paging is enabled
fn zero_frame(start: PhysicalAddress) {
    unsafe {
        let frame_start = start.as_u32() as *mut u8;
        let frame_slice = core::slice::from_raw_parts_mut(frame_start, super::physical::FRAME_SIZE);
        for i in 0..frame_slice.len() {
            frame_slice[i] = 0;
        }
    }
}

/// Modify CPU registers to enable paging
pub fn enable_paging() {
    unsafe {
        asm!(
            "push eax",
            "mov eax, cr0",
            "or eax, 0x80000000",
            "mov cr0, eax",
            "pop eax",
        );
    }
}

pub fn invalidate_page(addr: VirtualAddress) {
    let addr_raw: u32 = addr.into();
    unsafe {
        asm!(
            "invlpg [{0:e}]",
            in(reg) addr_raw,
        );
    }
}
