//! Simple freelist allocator backed by kernel map_memory syscall.
//!
//! Each allocated block has a header with its size. Free blocks are linked
//! together in a sorted freelist. Adjacent free blocks are merged on free().

use core::ptr;

use idos_api::syscall::memory::map_memory;

const PAGE_SIZE: usize = 4096;
const BLOCK_HEADER_SIZE: usize = core::mem::size_of::<BlockHeader>();
const MIN_ALLOC: usize = 16; // minimum allocation size (excluding header)

#[repr(C)]
struct BlockHeader {
    /// Size of the usable region (excludes this header)
    size: usize,
    /// If free, pointer to next free block. If allocated, 0.
    next_free: *mut BlockHeader,
}

/// Sentinel head of the free list
static mut FREE_LIST: *mut BlockHeader = ptr::null_mut();

/// Current break: the next address we can request pages from the kernel
static mut HEAP_END: usize = 0;

/// Base of the heap region
static mut HEAP_BASE: usize = 0;

pub fn init() {
    // Nothing to do yet; first malloc will request pages
}

/// Preferred starting address for the heap. High enough to avoid collisions
/// with ELF mappings, but low enough to have plenty of room to grow upward.
const HEAP_START_HINT: u32 = 0x8000_0000;

/// Request more memory from the kernel via map_memory.
/// The kernel always does closest-match for the requested virtual address,
/// so we always pass a hint and use whatever address comes back.
unsafe fn grow_heap(min_size: usize) -> *mut BlockHeader {
    let total_needed = min_size + BLOCK_HEADER_SIZE;
    let pages = (total_needed + PAGE_SIZE - 1) / PAGE_SIZE;
    let alloc_size = pages * PAGE_SIZE;

    let hint = if HEAP_END == 0 {
        HEAP_START_HINT
    } else {
        HEAP_END as u32
    };

    let addr = match map_memory(Some(hint), alloc_size as u32, None) {
        Ok(a) => a as usize,
        Err(()) => return ptr::null_mut(),
    };

    if HEAP_BASE == 0 {
        HEAP_BASE = addr;
    }
    let new_end = addr + alloc_size;
    if new_end > HEAP_END {
        HEAP_END = new_end;
    }

    let block = addr as *mut BlockHeader;
    (*block).size = alloc_size - BLOCK_HEADER_SIZE;
    (*block).next_free = ptr::null_mut();
    block
}

/// Insert a block into the free list, maintaining address order and merging
/// adjacent blocks.
unsafe fn insert_free(block: *mut BlockHeader) {
    if block.is_null() {
        return;
    }

    let block_addr = block as usize;
    let block_end = block_addr + BLOCK_HEADER_SIZE + (*block).size;

    // Find insertion point (sorted by address)
    let mut prev: *mut BlockHeader = ptr::null_mut();
    let mut curr = FREE_LIST;

    while !curr.is_null() && (curr as usize) < block_addr {
        prev = curr;
        curr = (*curr).next_free;
    }

    // Try merge with next block
    if !curr.is_null() && block_end == curr as usize {
        (*block).size += BLOCK_HEADER_SIZE + (*curr).size;
        (*block).next_free = (*curr).next_free;
    } else {
        (*block).next_free = curr;
    }

    // Try merge with previous block
    if !prev.is_null() {
        let prev_end = prev as usize + BLOCK_HEADER_SIZE + (*prev).size;
        if prev_end == block_addr {
            (*prev).size += BLOCK_HEADER_SIZE + (*block).size;
            (*prev).next_free = (*block).next_free;
        } else {
            (*prev).next_free = block;
        }
    } else {
        FREE_LIST = block;
    }
}

#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut u8 {
    if size == 0 {
        return ptr::null_mut();
    }

    // Align size up to 8 bytes
    let size = if size < MIN_ALLOC {
        MIN_ALLOC
    } else {
        (size + 7) & !7
    };

    // First-fit search
    let mut prev: *mut BlockHeader = ptr::null_mut();
    let mut curr = FREE_LIST;

    while !curr.is_null() {
        if (*curr).size >= size {
            // Found a fit
            let remaining = (*curr).size - size;
            if remaining > BLOCK_HEADER_SIZE + MIN_ALLOC {
                // Split: create a new free block after the allocation
                let new_free =
                    ((curr as usize) + BLOCK_HEADER_SIZE + size) as *mut BlockHeader;
                (*new_free).size = remaining - BLOCK_HEADER_SIZE;
                (*new_free).next_free = (*curr).next_free;
                (*curr).size = size;

                if !prev.is_null() {
                    (*prev).next_free = new_free;
                } else {
                    FREE_LIST = new_free;
                }
            } else {
                // Use the whole block
                if !prev.is_null() {
                    (*prev).next_free = (*curr).next_free;
                } else {
                    FREE_LIST = (*curr).next_free;
                }
            }

            (*curr).next_free = ptr::null_mut();
            return (curr as *mut u8).add(BLOCK_HEADER_SIZE);
        }
        prev = curr;
        curr = (*curr).next_free;
    }

    // No fit found, grow heap
    let block = grow_heap(size);
    if block.is_null() {
        return ptr::null_mut();
    }

    // The new block might be larger than needed; split if possible
    let remaining = (*block).size - size;
    if remaining > BLOCK_HEADER_SIZE + MIN_ALLOC {
        let new_free =
            ((block as usize) + BLOCK_HEADER_SIZE + size) as *mut BlockHeader;
        (*new_free).size = remaining - BLOCK_HEADER_SIZE;
        (*new_free).next_free = ptr::null_mut();
        (*block).size = size;
        insert_free(new_free);
    }

    (*block).next_free = ptr::null_mut();
    (block as *mut u8).add(BLOCK_HEADER_SIZE)
}

#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }

    let block = (ptr as *mut BlockHeader).sub(1);
    insert_free(block);
}

#[no_mangle]
pub unsafe extern "C" fn calloc(nmemb: usize, size: usize) -> *mut u8 {
    let total = nmemb.wrapping_mul(size);
    if total == 0 {
        return ptr::null_mut();
    }
    let p = malloc(total);
    if !p.is_null() {
        ptr::write_bytes(p, 0, total);
    }
    p
}

#[no_mangle]
pub unsafe extern "C" fn realloc(ptr: *mut u8, new_size: usize) -> *mut u8 {
    if ptr.is_null() {
        return malloc(new_size);
    }
    if new_size == 0 {
        free(ptr);
        return ptr::null_mut();
    }

    let block = (ptr as *mut BlockHeader).sub(1);
    let old_size = (*block).size;

    if new_size <= old_size {
        // Shrinking or same size: could split, but for simplicity just keep it
        return ptr;
    }

    // Need to grow: allocate new, copy, free old
    let new_ptr = malloc(new_size);
    if new_ptr.is_null() {
        return ptr::null_mut();
    }
    ptr::copy_nonoverlapping(ptr, new_ptr, old_size);
    free(ptr);
    new_ptr
}
