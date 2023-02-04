use alloc::boxed::Box;
use alloc::vec::Vec;
use spin::Mutex;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::physical::allocate_frame;
use crate::memory::virt::invalidate_page;
use crate::memory::virt::page_table::PageTable;
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

/// Stores a global bitmap of which kernel stacks have been allocated, so they
/// can easily be recycled
static STACK_ALLOCATION_BITMAP: Mutex<Vec<u8>> = Mutex::new(Vec::new());

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

/// Provided a numeric index of a kernel stack, construct a Boxed slice of
/// bytes that can be used to reference its stack area.
fn stack_box_from_index(index: usize) -> Box<[u8]> {
    let stack_bottom = KERNEL_STACKS_TOP - ((index + 1) * STACK_SIZE_IN_BYTES);
    let stack_ptr = stack_bottom as *mut u8;
    unsafe {
        alloc::vec::Vec::from_raw_parts(stack_ptr, STACK_SIZE_IN_BYTES, STACK_SIZE_IN_BYTES)
            .into_boxed_slice()
    }
}

/// Starting from the lowest index, search for an unallocated kernel stack
fn find_free_stack() -> usize {
    let mut stack_bitmap = STACK_ALLOCATION_BITMAP.lock();
    for (index, map) in stack_bitmap.iter_mut().enumerate() {
        let mut stack_index = index * 8;
        if *map == 0xff {
            continue;
        }
        let mut inverse = !*map;
        let mut mask = 1;
        while inverse != 0 {
            if inverse & 1 != 0 {
                *map |= mask;
                return stack_index;
            }
            inverse >>= 1;
            mask <<= 1;
            stack_index += 1;
        }
    }
    // No empty bit found
    let stack_index = stack_bitmap.len() * 8;
    stack_bitmap.push(1);
    stack_index
}

pub fn create_initial_stack() -> Box<[u8]> {
    let mut stack_bitmap = STACK_ALLOCATION_BITMAP.lock();
    stack_bitmap.push(1);
    stack_box_from_index(0)
}

pub fn free_stack(stack: Box<[u8]>) {
    let box_ptr = Box::into_raw(stack);
    // TODO: mark the stack as free and available for a new task
}

pub fn allocate_stack() -> Box<[u8]> {
    let index = find_free_stack();
    let stack = stack_box_from_index(index);
    let ptr: *const u8 = &stack[0];
    let stack_start = VirtualAddress::new(ptr as u32);
    let table_location = 0xffc00000 + 0x1000 * stack_start.get_page_directory_index();
    let page_table = PageTable::at_address(VirtualAddress::new(table_location as u32));
    let table_index = stack_start.get_page_table_index();
    let frame_address = allocate_frame().unwrap().to_physical_address();
    page_table.get_mut(table_index).set_address(frame_address);
    page_table.get_mut(table_index).set_present();
    invalidate_page(stack_start);
    
    stack
}

