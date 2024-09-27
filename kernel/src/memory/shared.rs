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

use crate::task::actions::memory::{map_memory_for_task, unmap_memory_for_task};
use crate::task::id::TaskID;
use crate::task::memory::{MemoryBacking, TaskMemoryError};
use crate::task::paging::get_current_physical_address;
use crate::task::switching::{get_current_id, get_current_task, get_task};
use super::address::{PhysicalAddress, VirtualAddress};

pub struct SharedMemoryRange {
    pub unmap_on_drop: bool,
    pub owner: TaskID,
    pub mapped_to: VirtualAddress,
    pub physical_frame: PhysicalAddress,
    pub range_offset: u32,
    pub range_length: u32,
}

impl SharedMemoryRange {
    pub fn for_slice<T>(slice: &[T]) -> Self {
        if slice.len() == 0 {
            panic!("Cannot create a shared memory range for an empty slice");
        }
        let start = slice.as_ptr() as u32;

        let mapped_to = VirtualAddress::new(start).prev_page_barrier();
        let physical_frame = get_current_physical_address(mapped_to).expect("Cannot share unpaged memory!");
        let range_offset = start - mapped_to.as_u32();
        let range_length = (slice.len() * core::mem::size_of::<T>()) as u32;

        let owner = get_current_id();

        Self {
            unmap_on_drop: false,
            owner,
            mapped_to,
            physical_frame,
            range_offset,
            range_length,
        }
    }

    pub fn for_struct<T>(s: &T) -> Self {
        let start = s as *const T as u32;

        let mapped_to = VirtualAddress::new(start).prev_page_barrier();
        let physical_frame = get_current_physical_address(mapped_to).expect("Cannot share unpaged memory!");
        let range_offset = start - mapped_to.as_u32();
        let range_length = core::mem::size_of::<T>() as u32;

        let owner = get_current_id();

        Self {
            unmap_on_drop: false,
            owner,
            mapped_to,
            physical_frame,
            range_offset,
            range_length,
        }
    }

    /// Map the page containing the range
    pub fn share_with_task(&self, id: TaskID) -> Self {
        let current_is_kernel = !get_current_task().read().has_executable();
        let dest_is_kernel = !get_task(id).unwrap().read().has_executable();
        if self.mapped_to >= VirtualAddress::new(0xc0000000) && current_is_kernel && dest_is_kernel {
            // two tasks sharing memory in the kernel space
            // Since all kernel memory is shared, there's no need to remap this
            Self {
                unmap_on_drop: false,
                owner: id,
                mapped_to: self.mapped_to,
                physical_frame: self.physical_frame,
                range_offset: self.range_offset,
                range_length: self.range_length,    
            }
        } else {
            let mapped_to = map_memory_for_task(id, None, 4096, MemoryBacking::Direct(self.physical_frame)).unwrap();

            crate::kprint!("SHARING to {:?}. {:?} / {:?} -> {:?}\n", id, self.mapped_to, mapped_to, self.physical_frame);

            Self {
                unmap_on_drop: true,
                owner: id,
                mapped_to,
                physical_frame: self.physical_frame,
                range_offset: self.range_offset,
                range_length: self.range_length,
            }
        }
    }

    pub fn get_range_start(&self) -> u32 {
        self.mapped_to.as_u32() + self.range_offset
    }

    /// Turn the shared area back into a slice of type T.
    /// Assuming the type is the same as the original shared range, there
    /// should not be a problem.
    pub fn try_as_slice<T>(&self) -> Option<&mut [T]> {
        let element_size = core::mem::size_of::<T>() as u32;
        if self.range_length % element_size != 0 {
            // not perfectly divisible into a slice of T's
            return None;
        }
        let len = self.range_length / element_size;

        let start_ptr = self.get_range_start() as *mut T;
        unsafe { Some(core::slice::from_raw_parts_mut(start_ptr, len as usize)) }
    }
}

impl Drop for SharedMemoryRange {
    fn drop(&mut self) {
        if !self.unmap_on_drop {
            return;
        }
        crate::kprint!("SHARE: Unmap {:?} for {:?}, no longer in use\n", self.mapped_to, self.owner);

        match unmap_memory_for_task(self.owner, self.mapped_to, 4096) {
            Err(TaskMemoryError::NoTask) => crate::kprint!("Task already dropped, no need to unmap\n"),
            Err(e) => panic!("{:?}", e),
            Ok(_) => ()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::task::{
        actions::{
            memory::map_memory,
            lifecycle::{create_kernel_task, terminate, wait_for_child},
            read_message_blocking, send_message,
        },
        memory::MemoryBacking, messaging::Message,
    };
    use super::SharedMemoryRange;

    #[test_case]
    fn sharing_within_kernel() {
    }

    #[test_case]
    fn sharing_outside_kernel() {
        // create a buffer
        let addr = map_memory(None, 4096, MemoryBacking::Anonymous).unwrap();
        let mut buffer = unsafe {
            core::slice::from_raw_parts_mut(addr.as_u32() as *mut u8, 4096)
        };
        for i in 0..10 {
            buffer[i] = 0;
        }

        let child = create_kernel_task(outside_kernel_subtask, Some("CHILD"));
        let range = SharedMemoryRange::for_slice(&buffer[0..10]);
        let shared = range.share_with_task(child);

        let mut msg = Message::empty();
        msg.args[0] = shared.get_range_start();
        msg.args[1] = shared.range_length;

        send_message(
            child,
            msg,
            0xffffffff,
        );

        wait_for_child(child, None);

        for i in 0..10 {
            assert_eq!(buffer[i], i as u8);
        }
    }

    fn outside_kernel_subtask() -> ! {
        let (message_read, _) = read_message_blocking(None);
        let packet = message_read.unwrap();
        let (_, message) = packet.open();
        let addr = message.args[0];
        let size = message.args[1] as usize;
        let mut buffer = unsafe {
            core::slice::from_raw_parts_mut(addr as *mut u8, size)
        };
        for i in 0..10 {
            buffer[i] = i as u8;
        }
        terminate(0);
    }
}

