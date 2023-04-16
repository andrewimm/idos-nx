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

use crate::task::id::TaskID;
use crate::task::switching::get_current_id;
use super::address::{PhysicalAddress, VirtualAddress};

pub struct SharedMemoryRange {
    pub owner: TaskID,
    pub mapped_to: VirtualAddress,
    pub physical_frame: PhysicalAddress,
    pub range_offset: u32,
    pub range_length: u32,
}

impl SharedMemoryRange {
    pub fn for_slice<T>(slice: &[T]) -> Self {
        let start = slice.as_ptr() as u32;

        let mapped_to = VirtualAddress::new(start).prev_page_barrier();
        // TODO: find this from the mapping table
        let physical_frame = PhysicalAddress::new(0);
        let range_offset = start - mapped_to.as_u32();
        let range_length = (slice.len() * core::mem::size_of::<T>()) as u32;

        let owner = get_current_id();

        Self {
            owner,
            mapped_to,
            physical_frame,
            range_offset,
            range_length,
        }
    }

    /// Map the page containing the range
    pub fn share_with_task(&self, id: TaskID) -> Self {
        if self.mapped_to < VirtualAddress::new(0xc0000000) {
            panic!("Shared memory ranges are only supported in the kernel for now!");
        } else {
            // in the kernel space, all memory is shared, so nothing needs to
            // be mapped or unmapped. Just create a new instance of the struct.
            Self {
                owner: id,
                mapped_to: self.mapped_to,
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
