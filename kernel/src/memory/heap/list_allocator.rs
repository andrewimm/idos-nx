use alloc::alloc::Layout;
use core::ptr::null_mut;

use crate::memory::address::VirtualAddress;
use crate::memory::physical::allocate_frame;
use crate::task::paging::{current_pagedir_map, PermissionFlags};

/// Allocator using a linked list of free blocks to easily find available
/// space. To avoid too much overhead in memory, most of the properties are
/// stored as 32-bit pointers.
/// Both free and allocated nodes start with a 32-bit magic number, and a
/// 32-bit value indicating their size (including the 8-byte header). A free
/// node stores a 32-bit pointer to the next free nodes in the following 4
/// bytes. The rest of the free node is unpredictable:
///
///     FREE NODE:
///     | magic | size | next | random bytes ........ |
/// 
/// An allocated node needs to store some size data for when it is deallocated.
/// We need to know how much space after the pointer was allocated in the
/// initial alloc call. There may also be some extra padding before the
/// pointer, since alloc passes layout / alignment requirements.
/// An allocated node stores the magic number and the total size in the first
/// 8 bytes, then has an area for alignment padding. The padding is at least
/// 4 bytes long -- the last 4 bytes store the size of the padding. Following
/// the padding is the actual data area, to which the allocated pointer refers.
/// 
///     ALLOCATED NODE:
///     | magic | size | padding ... | padding_size | data ......... |
///
/// When an allocated node is deallocated, we track backwards to the previous
/// 4-byte value to find the padding size. Subtracting that value from the
/// pointer, we find the start of the block, and can determine its actual size.
pub struct ListAllocator {
    start: usize,
    size: usize,
    first_free: usize,
}

/// Magic number indicating a memory node: "ALLOCATE"
const MAGIC: u32 = 0xA110CA7E;
/// Size of the magic + size header
const HEADER_SIZE: usize = core::mem::size_of::<u32>() * 2;

#[repr(C, packed)]
pub struct AllocNode {
    /// Magic number used to confirm we're looking at a real allocation node
    magic: u32,
    /// Size of the node, including the magic + size fields.
    /// The lower 31 bits are used for the actual size. The 32nd bit is used
    /// to indicate if the node is currently in use (1) or free (0).
    size: u32,
    /// In an allocated node, the data would start here. In a free node, this
    /// offset contains a pointer to the next free node.
    next: u32,
}

impl AllocNode {
    pub fn init(&mut self, size: usize) {
        self.magic = MAGIC;
        self.size = (size & 0x7fffffff) as u32;
        self.next = 0;
    }

    pub fn is_valid(&self) -> bool {
        self.magic == MAGIC
    }

    pub fn get_size(&self) -> usize {
        (self.size & 0x7fffffff) as usize
    }

    pub fn is_free(&self) -> bool {
        self.size & 0x80000000 == 0
    }

    pub fn get_next(&self) -> usize {
        self.next as usize
    }

    pub fn mark_occupied(&mut self) {
        self.size = self.size | 0x80000000;
    }

    pub fn mark_free(&mut self) {
        self.size = self.size & 0x7fffffff;
    }

    pub fn set_next(&mut self, addr: usize) {
        self.next = addr as u32;
    }

    pub fn set_size(&mut self, size: usize) {
        self.size = (self.size & 0x80000000) | (size as u32 & 0x7fffffff);
    }
}

pub fn get_aligned_start(node: *const AllocNode, alignment: usize) -> usize {
    let start = (node as usize) + 12;
    let low_alignment_bits = alignment - 1;
    (start + low_alignment_bits) & !low_alignment_bits
}

pub unsafe fn mark_padded(node: *const AllocNode, padding: usize) {
    if padding < 4 {
        panic!("Padding must be at least 4 bytes");
    }
    let start = (node as usize) + HEADER_SIZE;
    let padding_offset = start + padding - 4;
    let padding_ptr = padding_offset as *mut u32;
    *padding_ptr = padding as u32;
}

pub unsafe fn find_node_from_allocated_ptr(ptr: *mut u8) -> *mut AllocNode {
    let addr = ptr as usize;
    let padding_addr = addr - 4;
    let padding_ptr = padding_addr as *const u32;
    let padding_size = (*padding_ptr) as usize;
    (addr - padding_size - HEADER_SIZE) as *mut AllocNode
}

impl ListAllocator {
    pub const fn empty() -> Self {
        Self {
            start: 0,
            size: 0,
            first_free: 0,
        }
    }

    pub fn new(start: usize, size: usize) -> Self {
        unsafe {
            let free_node = &mut *(start as *mut AllocNode);
            free_node.init(size);
        }
        Self {
            start,
            size,
            first_free: start,
        }
    }

    pub fn find_last_node(&self) -> &mut AllocNode {
        let mut current = self.first_free;
        loop {
            let node_ptr = current as *mut AllocNode;
            let node = unsafe { &mut *node_ptr };
            if node.next == 0 {
                return node;
            }
            current = node.next as usize;
        }
    }

    pub fn expand(&mut self, page_count: usize) {
        let prev_end = VirtualAddress::new((self.start + self.size) as u32);
        let expanded_bytes = 0x1000 * page_count;
        let last_node = self.find_last_node();
        last_node.size += expanded_bytes as u32;
        self.size += expanded_bytes;
        for i in 0..page_count {
            let page_start = prev_end + (i as u32 * 0x1000);
            let frame = allocate_frame().unwrap();
            current_pagedir_map(frame, page_start, PermissionFlags::empty());
        }
        crate::kprint!("Heap expanded by {} pages\n", page_count);
    }

    pub unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let mut prev = 0;
        let mut current = self.first_free;
        while current != 0 {
            let node_ptr = current as *mut AllocNode;
            let node = &mut *node_ptr;
            let next = node.next;
            let aligned_start = get_aligned_start(node_ptr, layout.align());
            let aligned_end = aligned_start + layout.size();
            if current + node.get_size() >= aligned_end {
                // Found an appropriately sized chunk
                let remainder = current + node.get_size() - aligned_end;
                node.mark_occupied();
                let padding = aligned_start - current - HEADER_SIZE;
                mark_padded(node_ptr, padding);

                if remainder > 16 {
                    // Turn the trailing space into a new free node
                    // Make sure it's aligned to 4 bytes
                    let trailing_start = (aligned_end + 3) & !3;
                    let new_node_ptr = trailing_start as *mut AllocNode;
                    let new_node = &mut *new_node_ptr;
                    let new_size = current + node.get_size() - trailing_start;
                    new_node.init(new_size);
                    new_node.next = next;
                    if prev != 0 {
                        let prev_node_ptr = prev as *mut AllocNode;
                        let prev_node = &mut *prev_node_ptr;
                        prev_node.next = trailing_start as u32;
                    } else {
                        // We split the first node, so update the head of the
                        // list
                        self.first_free = trailing_start as usize;
                    }

                    node.set_size(trailing_start - current);
                } else if next == 0 {
                    // if next is 0, we cannot have the start of the list be
                    // an invalid pointer
                    let end = self.start + self.size;
                    self.size += 0x1000;
                    let new_empty = end as *mut AllocNode;
                    (&mut *new_empty).init(0x1000);
                    self.first_free = end;
                    crate::kprint!("Heap expanded (B)\n");
                } else {
                    self.first_free = next as usize;
                }
                return aligned_start as *mut u8;
            }
            prev = current;
            current = next as usize;
        }
        null_mut()
    }

    pub unsafe fn dealloc(&mut self, ptr: *mut u8) {
        let addr = ptr as usize;
        if addr < self.start || addr > self.start + self.size {
            panic!("Attempted to dealloc out-of-bounds pointer");
        }
        let node_ptr = find_node_from_allocated_ptr(ptr);
        let node = &mut *node_ptr;
        if !node.is_valid() {
            panic!("Attempted to dealloc non-node");
        }
        if node.is_free() {
            panic!("Attempted to dealloc free node");
        }
        node.mark_free();

        // Add the node back into the free list to be used again
        let node_addr = node_ptr as usize;
        if node_addr < self.first_free {
            node.set_next(self.first_free);
            self.first_free = node_addr;
        } else {
            let mut iter_addr = self.first_free;
            while iter_addr != 0 {
                let iter_ptr = iter_addr as *mut AllocNode;
                let iter_node = &mut *iter_ptr;
                let next_addr = iter_node.get_next();
                if iter_addr < node_addr && (node_addr < next_addr || next_addr == 0) {
                    // insert the new free node here
                    iter_node.set_next(node_addr);
                    node.set_next(next_addr);
                    iter_addr = 0;
                } else {
                    iter_addr = next_addr;
                }
            }
        }

        self.merge_free_areas();
    }

    /// Iterate over the linked list of free nodes. If two adjacent nodes are
    /// free, merge them into a single free space.
    pub unsafe fn merge_free_areas(&mut self) {
        let mut iter_addr = self.first_free;
        while iter_addr != 0 {
            let iter_ptr = iter_addr as *mut AllocNode;
            let iter_node = &mut *iter_ptr;
            let next_byte = iter_addr + iter_node.get_size();
            let next_addr = iter_node.get_next();
            if next_byte == next_addr {
                // The two nodes are adjacent, and should be merged
                let next_ptr = next_addr as *mut AllocNode;
                let next_node = &mut *next_ptr;
                iter_node.set_size(iter_node.get_size() + next_node.get_size());
                iter_node.set_next(next_node.get_next());
            } else {
                iter_addr = next_addr;
            }
        }
    }
}
