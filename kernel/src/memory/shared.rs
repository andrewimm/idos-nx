//! Memory Safety can refer to a number of different OS concepts. This kernel
//! embraces certain of these, and ignores others. The kernel does what it can
//! to prevent programs from accidentally modifying each other or the host
//! system -- this is one of the main improvements over legacy DOS, where
//! everything runs in a shared memory space. However, there are times where
//! you do want to share memory between tasks. To allow this, the kernel
//! provides mechanisms for one program to deliberately modify the memory of
//! other programs.
//!
//! With shared memory, the kernel can grant one task access to memory that
//! was originally mapped into another task. This speeds up sharing of large
//! buffers, since they don't need to be copied through the kernel to new
//! memory first.
//!
//! Shared memory can only be implemented at page-level granularity. This means
//! that a program may indicate that it only wants to share a small area of
//! memory, but the entire page containing that range will be made available to
//! the other task. The receiving task must be trusted to not mess with
//! anything outside of that explicitly shared range.

use spin::Mutex;

use super::address::{PhysicalAddress, VirtualAddress};
use super::virt::page_iter::PageIter;
use crate::collections::RefCountMap;
use crate::task::actions::memory::{map_memory_for_task, unmap_memory_for_task};
use crate::task::id::TaskID;
use crate::task::map::get_task;
use crate::task::memory::MemoryBacking;
use crate::task::paging::get_current_physical_address;
use crate::task::switching::get_current_id;

static SHARED_MEMORY_REFCOUNT: Mutex<RefCountMap<PhysicalAddress>> = Mutex::new(RefCountMap::new());

/// Share an area of memory with another task. This is used for all IPC memory
/// sharing. It is also leveraged for zero-copy IO for drivers.
pub fn share_buffer(task: TaskID, vaddr: VirtualAddress, byte_size: usize) -> VirtualAddress {
    let total_pages = {
        let start = vaddr.prev_page_barrier();
        let end = (vaddr + byte_size as u32).next_page_barrier();
        let length = end - start;
        length / 0x1000
    };

    let mapping_start = {
        let task_lock = get_task(task).expect("Task does not exist");
        let available_space = task_lock
            .write()
            .memory_mapping
            .find_free_mapping_space(total_pages * 0x1000)
            .expect("Could not find contiguous space in task");
        available_space
    };
    let mut offset = 0;
    for page_start in PageIter::for_vaddr_range(vaddr, byte_size) {
        let frame_start =
            get_current_physical_address(page_start).expect("Cannot share unmapped memory");
        SHARED_MEMORY_REFCOUNT.lock().add_reference(frame_start);

        let mapped_offset = mapping_start + offset;
        map_memory_for_task(
            task,
            Some(mapped_offset),
            0x1000,
            MemoryBacking::Direct(frame_start),
        )
        .unwrap();
        super::LOGGER.log(format_args!(
            "SHARE: Map {:?} to {:?} for {:?}",
            mapped_offset, frame_start, task
        ));
        offset += 0x1000;
    }
    let mapping_offset = vaddr.as_u32() & 0xfff;
    mapping_start + mapping_offset
}

/// helper function for sharing a string between tasks, fetching the buffer
/// location and size and passing it to share_buffer
pub fn share_string(task_id: TaskID, s: &str) -> VirtualAddress {
    let string_addr = VirtualAddress::new(s.as_ptr() as u32);
    let string_len = s.len();
    share_buffer(task_id, string_addr, string_len)
}

pub fn release_buffer(vaddr: VirtualAddress, byte_size: usize) {
    let cur_task = get_current_id();
    super::LOGGER.log(format_args!(
        "SHARE: Release buffer as {:?} for {:?}",
        vaddr, cur_task
    ));
    for page_start in PageIter::for_vaddr_range(vaddr, byte_size) {
        let frame_start =
            get_current_physical_address(page_start).expect("Cannot release unmapped memory");
        let count_remaining = SHARED_MEMORY_REFCOUNT.lock().remove_reference(frame_start);
        // TODO: this could just be one unmap call. If there are multiple pages
        // this will make too many unnecessary calls
        unmap_memory_for_task(cur_task, page_start, 0x1000).unwrap();
        if count_remaining == 0 {
            super::LOGGER.log(format_args!("SHARE: Release frame {:?}", frame_start));
            // TODO: release the frame
        }
    }
}

#[cfg(test)]
mod tests {
    use super::share_buffer;
    use crate::task::{
        actions::{
            handle::{create_kernel_task, open_message_queue},
            io::{read_struct_sync, read_sync},
            lifecycle::terminate,
            memory::map_memory,
            send_message,
        },
        memory::MemoryBacking,
        messaging::Message,
    };

    #[test_case]
    fn sharing_within_kernel() {}

    #[test_case]
    fn sharing_outside_kernel() {
        // code for secondary task
        fn outside_kernel_subtask() -> ! {
            let mut message = Message::empty();
            let message_queue = open_message_queue();
            let _ = read_struct_sync(message_queue, &mut message, 0);
            let addr = message.args[0];
            let size = message.args[1] as usize;
            let buffer = unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, size) };
            for i in 0..10 {
                buffer[i] = i as u8;
                buffer[i + 0x200] = i as u8;
                buffer[i + 0x11f0] = i as u8;
            }
            terminate(0);
        }

        // create a 3-page mapping
        let addr = map_memory(None, 0x1000 * 3, MemoryBacking::Anonymous).unwrap();
        // create a buffer that extends across all three pages
        //       [ buffer........ ]
        // [ PAGE 0 ][ PAGE 1 ][ PAGE 2 ]
        let buffer =
            unsafe { core::slice::from_raw_parts_mut((addr.as_u32() + 0xf00) as *mut u8, 0x1200) };
        for i in 0..0x1200 {
            buffer[i] = 0;
        }

        let (child_handle, child_id) = create_kernel_task(outside_kernel_subtask, Some("CHILD"));

        let buffer_start = share_buffer(child_id, addr + 0xf00, buffer.len());

        let mut msg = Message::empty();
        msg.args[0] = buffer_start.as_u32();
        msg.args[1] = buffer.len() as u32;
        send_message(child_id, msg, 0xffffffff);

        let _ = read_sync(child_handle, &mut [0u8], 0);

        for i in 0..10 {
            assert_eq!(buffer[i], i as u8);
            assert_eq!(buffer[i + 0x200], i as u8);
            assert_eq!(buffer[i + 0x11f0], i as u8);
        }
    }
}
