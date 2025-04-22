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
pub mod page_iter;
pub mod page_table;
pub mod scratch;

use crate::task::stack::get_initial_kernel_stack_location;
use core::arch::asm;
use core::ops::Range;
use page_table::{PageDirectoryReference, PageTable};

use self::scratch::SCRATCH_PAGE_COUNT;

use super::address::{PhysicalAddress, VirtualAddress};
use super::physical::allocate_frame;

/// Create the initial page directory need to enable paging.
pub fn create_initial_pagedir(
    kernel_range: Range<VirtualAddress>,
    bios_memmap_address: PhysicalAddress,
) -> PageDirectoryReference {
    let dir_address = allocate_frame().unwrap().to_physical_address();
    zero_frame(dir_address);

    let dir = PageTable::at_address(VirtualAddress::new(dir_address.into()));
    // Point the last entry to itself, so that it is always accessible
    dir.get_mut(1023).set_address(dir_address);
    dir.get_mut(1023).set_present();

    // Identity-map the kernel
    {
        let first_dir_index = kernel_range.start.get_page_directory_index();
        let last_dir_index = kernel_range.end.get_page_directory_index();
        for dir_index in first_dir_index..=last_dir_index {
            let table_frame = allocate_frame().unwrap().to_physical_address();
            zero_frame(table_frame);
            dir.get_mut(dir_index).set_address(table_frame);
            dir.get_mut(dir_index).set_present();

            let table = PageTable::at_address(VirtualAddress::new(table_frame.into()));
            /*
            let first_table_index = if dir_index == first_dir_index {
                initial_range.start.get_page_table_index()
            } else {
                0
            };
            */
            let first_table_index = 0;
            let last_table_index = if dir_index == last_dir_index {
                kernel_range.end.get_page_table_index()
            } else {
                1023
            };
            for table_index in first_table_index..=last_table_index {
                let identity_map = PhysicalAddress::new(
                    dir_index as u32 * 0x400 * 0x1000 + table_index as u32 * 0x1000,
                );
                table.get_mut(table_index).set_address(identity_map);
                table.get_mut(table_index).set_present();
            }

            // Copy the same table to high memory, so that the kernel is
            // accessible above 0xc0000000
            dir.get_mut(dir_index + 0x300).set_address(table_frame);
            dir.get_mut(dir_index + 0x300).set_present();
        }
    }

    // Iterate over the BIOS memory map and map all of the Reserved sections.
    // These are used for BIOS values or ACPI tables, and the init process needs
    // to be able to read them in order to extract data about system hardware
    {
        // This only works because we previously marked the frame with the memory
        // map as occupied, so it shouldn't have been overwritten.
        // Once we're done reading it, we can free the frame. It's no longer
        // ncecessary.
        let memory_map = super::physical::bios::load_memory_map(bios_memmap_address);
        for entry in memory_map.iter() {
            if !entry.is_reserved() {
                continue;
            }
            let start = VirtualAddress::new(entry.get_base());
            let end = start + entry.get_length() - 1;

            let first_page = start.prev_page_barrier();
            let last_page = end.prev_page_barrier();
            crate::kprintln!("MAP FROM {:?} TO {:?}", first_page, last_page);
            let mut page_start = first_page;
            while page_start <= last_page {
                let dir_index = page_start.get_page_directory_index();
                let table_index = page_start.get_page_table_index();
                let table_address = if dir.get(dir_index).is_present() {
                    dir.get(dir_index).get_address()
                } else {
                    let table_frame = allocate_frame().unwrap().to_physical_address();
                    zero_frame(table_frame);
                    dir.get_mut(dir_index).set_address(table_frame);
                    dir.get_mut(dir_index).set_present();
                    table_frame
                };

                let table = PageTable::at_address(VirtualAddress::new(table_address.into()));
                if !table.get(table_index).is_present() {
                    table
                        .get_mut(table_index)
                        .set_address(PhysicalAddress::new(page_start.into()));
                    table.get_mut(table_index).set_present();
                }

                page_start = page_start + 0x1000;
            }
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

    PageDirectoryReference::new(dir_address)
}

/// Zero out an allocated frame, does not work once paging is enabled
fn zero_frame(start: PhysicalAddress) {
    unsafe {
        let frame_start = start.as_u32() as *mut u8;
        let frame_slice =
            core::slice::from_raw_parts_mut(frame_start, super::physical::FRAME_SIZE as usize);
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
