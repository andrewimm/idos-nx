use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, Ordering};

use idos_api::syscall::memory::map_memory;

// Currently implemented as a simple slab allocator with 6 size classes.
// With alignment, 16 would be too small, and 2048 would be wasting half a page.

const NUM_CLASSES: usize = 6;
const SIZE_CLASSES: [usize; NUM_CLASSES] = [32, 64, 128, 256, 512, 1024];
const PAGE_SIZE: usize = 0x1000;
const FREE_END: u16 = 0xFFFF;

/// Find the size-class index for an allocation of `effective` bytes,
/// or `None` if it exceeds the largest class.
fn class_index(effective: usize) -> Option<usize> {
    SIZE_CLASSES.iter().position(|&s| s >= effective)
}

/// SlabHeader lives in slot 0 of every slab page. It contains metadata about
/// the slab and links for the partial and all slabs lists.
/// The free list is stored as an index-based linked list in the slots
/// themselves, using the u16 at the start of each free slot to point to the
/// next free slot (or 0xFFFF for end).
/// Total size: 16 bytes, which fits in the minimum 32-byte slot size.
#[repr(C)]
struct SlabHeader {
    next_partial: *mut SlabHeader, // 4B — partial-list link
    next_all: *mut SlabHeader,     // 4B — all-slabs list link
    free_head: u16,                // 2B — index of first free slot (0xFFFF = none)
    used_count: u16,               // 2B
    total_slots: u16,              // 2B
    class_index: u8,               // 1B
    _pad: [u8; 1],                 // 1B
}

/// For large allocations, we need to track the allocated regions so we can free
/// them later. This struct is used for that tracking.
#[repr(C)]
struct LargeAllocEntry {
    addr: u32,
    size: u32,
    next: *mut LargeAllocEntry,
}

#[repr(C)]
struct FreePage {
    addr: u32,
    size: u32,
    next: *mut FreePage,
}

/// The SlabAllocator struct contains all the global state for the allocator,
/// including the slab lists, linked-list of large allocs, and linked-list of
/// free pages. A single global lock protects all of this state; idos is an
/// inherently single-threaded system so we don't need anything more
/// sophisticated.
struct SlabAllocator {
    partial_slabs: [*mut SlabHeader; NUM_CLASSES],
    all_slabs: [*mut SlabHeader; NUM_CLASSES],
    large_allocs: *mut LargeAllocEntry,
    free_pages: *mut FreePage,
    lock: AtomicBool,
}

// since we use a sentinel lock intead of a more Rust-y style with guards,
// we need to manually assert these
unsafe impl Send for SlabAllocator {}
unsafe impl Sync for SlabAllocator {}

impl SlabAllocator {
    const fn new() -> Self {
        Self {
            partial_slabs: [core::ptr::null_mut(); NUM_CLASSES],
            all_slabs: [core::ptr::null_mut(); NUM_CLASSES],
            large_allocs: core::ptr::null_mut(),
            free_pages: core::ptr::null_mut(),
            lock: AtomicBool::new(false),
        }
    }

    fn acquire(&self) {
        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    fn release(&self) {
        self.lock.store(false, Ordering::Release);
    }

    /// Get a zeroed page. Tries free_pages first, then map_memory.
    unsafe fn acquire_page(&mut self) -> *mut u8 {
        // Check free_pages for a single-page entry
        let mut prev: *mut FreePage = core::ptr::null_mut();
        let mut cur = self.free_pages;
        while !cur.is_null() {
            let entry = &mut *cur;
            if entry.size as usize == PAGE_SIZE {
                // Exact single page — unlink and use
                if prev.is_null() {
                    self.free_pages = entry.next;
                } else {
                    (*prev).next = entry.next;
                }
                let addr = entry.addr as *mut u8;
                // Free the FreePage tracking node back to 32B slab
                self.slab_dealloc(cur as *mut u8, core::mem::size_of::<FreePage>());
                // Zero the page
                core::ptr::write_bytes(addr, 0, PAGE_SIZE);
                return addr;
            }
            // If the free region is larger, carve off one page
            if entry.size as usize >= PAGE_SIZE {
                let addr = entry.addr as *mut u8;
                entry.addr += PAGE_SIZE as u32;
                entry.size -= PAGE_SIZE as u32;
                if entry.size == 0 {
                    // Remove empty entry
                    if prev.is_null() {
                        self.free_pages = entry.next;
                    } else {
                        (*prev).next = entry.next;
                    }
                    self.slab_dealloc(cur as *mut u8, core::mem::size_of::<FreePage>());
                }
                core::ptr::write_bytes(addr, 0, PAGE_SIZE);
                return addr;
            }
            prev = cur;
            cur = entry.next;
        }

        // No reusable page — map fresh
        let addr = map_memory(None, PAGE_SIZE as u32, None).unwrap_or(0xFFFF_FFFF);
        if addr == 0xFFFF_FFFF {
            return core::ptr::null_mut();
        }
        core::ptr::write_bytes(addr as *mut u8, 0, PAGE_SIZE);
        addr as *mut u8
    }

    /// Create a new slab page for the given class index, initialize its header
    /// and free list, and link it into the all_slabs and partial_slabs lists.
    /// Returns a pointer to the new slab's header, or null on failure.
    unsafe fn new_slab_page(&mut self, ci: usize) -> *mut SlabHeader {
        let page = self.acquire_page();
        if page.is_null() {
            return core::ptr::null_mut();
        }

        let slot_size = SIZE_CLASSES[ci];
        let total_slots = (PAGE_SIZE / slot_size) - 1; // slot 0 is header

        // Write header in slot 0
        let header = page as *mut SlabHeader;
        (*header).next_partial = core::ptr::null_mut();
        (*header).next_all = core::ptr::null_mut();
        (*header).free_head = 1; // first usable slot
        (*header).used_count = 0;
        (*header).total_slots = total_slots as u16;
        (*header).class_index = ci as u8;

        // Build inline free list: slot 1 → 2 → ... → N → 0xFFFF
        for i in 1..=total_slots {
            let slot_ptr = page.add(i * slot_size) as *mut u16;
            if i < total_slots {
                *slot_ptr = (i + 1) as u16;
            } else {
                *slot_ptr = FREE_END;
            }
        }

        // Link into all_slabs list
        (*header).next_all = self.all_slabs[ci];
        self.all_slabs[ci] = header;

        // Link into partial list
        (*header).next_partial = self.partial_slabs[ci];
        self.partial_slabs[ci] = header;

        header
    }

    /// Allocates a slot from a slab of the given class index. Returns a pointer
    /// to the allocated slot, or null on failure.
    unsafe fn slab_alloc(&mut self, ci: usize) -> *mut u8 {
        // Get a partial slab (or create one)
        let mut slab = self.partial_slabs[ci];
        if slab.is_null() {
            slab = self.new_slab_page(ci);
            if slab.is_null() {
                return core::ptr::null_mut();
            }
        }

        let header = &mut *slab;
        let slot_size = SIZE_CLASSES[ci];
        let page_base = slab as *mut u8;

        // Pop from free list
        let slot_idx = header.free_head;
        debug_assert!(slot_idx != FREE_END);
        let slot_ptr = page_base.add(slot_idx as usize * slot_size);
        header.free_head = *(slot_ptr as *const u16);
        header.used_count += 1;

        // If slab is now full, unlink from partial list
        if header.free_head == FREE_END {
            self.partial_slabs[ci] = header.next_partial;
            header.next_partial = core::ptr::null_mut();
        }

        slot_ptr
    }

    /// Deallocates a slot at the given pointer with the given effective size.
    /// The effective size is used to determine the class index and thus the
    /// slot size and layout.
    unsafe fn slab_dealloc(&mut self, ptr: *mut u8, effective: usize) {
        let ci = class_index(effective).unwrap();
        let slot_size = SIZE_CLASSES[ci];
        let page_base = (ptr as usize & !0xFFF) as *mut u8;
        let header = page_base as *mut SlabHeader;
        let slot_idx = (ptr as usize - page_base as usize) / slot_size;

        let was_full = (*header).free_head == FREE_END;

        // Push onto free list
        *(ptr as *mut u16) = (*header).free_head;
        (*header).free_head = slot_idx as u16;
        (*header).used_count -= 1;

        // If slab was full, re-link to partial list
        if was_full {
            let ci = (*header).class_index as usize;
            (*header).next_partial = self.partial_slabs[ci];
            self.partial_slabs[ci] = header;
        }
    }

    /// Allocates a large block of memory (greater than 1024 bytes) by finding
    /// free pages or mapping new ones. Returns a pointer to the allocated block,
    /// or null on failure.
    unsafe fn large_alloc(&mut self, size: usize) -> *mut u8 {
        let alloc_size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

        // Try free_pages for first fit
        let mut prev: *mut FreePage = core::ptr::null_mut();
        let mut cur = self.free_pages;
        while !cur.is_null() {
            let entry = &mut *cur;
            if entry.size as usize >= alloc_size {
                let addr = entry.addr;
                let remainder = entry.size as usize - alloc_size;
                if remainder > 0 {
                    entry.addr += alloc_size as u32;
                    entry.size = remainder as u32;
                } else {
                    // Remove entry
                    if prev.is_null() {
                        self.free_pages = entry.next;
                    } else {
                        (*prev).next = entry.next;
                    }
                    self.slab_dealloc(cur as *mut u8, core::mem::size_of::<FreePage>());
                }
                // Record in large_allocs
                self.record_large_alloc(addr, alloc_size as u32);
                return addr as *mut u8;
            }
            prev = cur;
            cur = entry.next;
        }

        // No fit — map fresh
        let addr = map_memory(None, alloc_size as u32, None).unwrap_or(0xFFFF_FFFF);
        if addr == 0xFFFF_FFFF {
            return core::ptr::null_mut();
        }
        self.record_large_alloc(addr, alloc_size as u32);
        addr as *mut u8
    }

    /// Records a large allocation in the large_allocs linked list for tracking.
    unsafe fn record_large_alloc(&mut self, addr: u32, size: u32) {
        // Allocate a LargeAllocEntry from the 32B slab
        let entry_ptr = self.slab_alloc(0) as *mut LargeAllocEntry;
        (*entry_ptr).addr = addr;
        (*entry_ptr).size = size;
        (*entry_ptr).next = self.large_allocs;
        self.large_allocs = entry_ptr;
    }

    /// Deallocates a large block of memory, putting it into the free page list
    /// and removing its tracking entry from large_allocs. The pointer must have
    /// been returned by a previous call to large_alloc.
    unsafe fn large_dealloc(&mut self, ptr: *mut u8) {
        let target = ptr as u32;
        let mut prev: *mut LargeAllocEntry = core::ptr::null_mut();
        let mut cur = self.large_allocs;
        while !cur.is_null() {
            if (*cur).addr == target {
                let size = (*cur).size;
                // Unlink
                if prev.is_null() {
                    self.large_allocs = (*cur).next;
                } else {
                    (*prev).next = (*cur).next;
                }
                // Free the tracking entry
                self.slab_dealloc(cur as *mut u8, core::mem::size_of::<LargeAllocEntry>());
                // Add pages to free_pages
                self.add_free_pages(target, size);
                return;
            }
            prev = cur;
            cur = (*cur).next;
        }
        // Not found — ignore (shouldn't happen)
    }

    unsafe fn add_free_pages(&mut self, addr: u32, size: u32) {
        let entry_ptr = self.slab_alloc(0) as *mut FreePage;
        (*entry_ptr).addr = addr;
        (*entry_ptr).size = size;
        (*entry_ptr).next = self.free_pages;
        self.free_pages = entry_ptr;
    }
}

static mut SLAB: SlabAllocator = SlabAllocator::new();

struct AllocatorWrapper;

// Implement GlobalAlloc so we can use the Rust allocation APIs.
unsafe impl GlobalAlloc for AllocatorWrapper {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let effective = layout.size().max(layout.align());
        SLAB.acquire();
        let slab = &mut *core::ptr::addr_of_mut!(SLAB);
        let result = match class_index(effective) {
            Some(ci) => slab.slab_alloc(ci),
            None => slab.large_alloc(layout.size()),
        };
        SLAB.release();
        result
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let effective = layout.size().max(layout.align());
        SLAB.acquire();
        let slab = &mut *core::ptr::addr_of_mut!(SLAB);
        if effective > 1024 {
            slab.large_dealloc(ptr);
        } else {
            slab.slab_dealloc(ptr, effective);
        }
        SLAB.release();
    }
}

#[global_allocator]
static ALLOC: AllocatorWrapper = AllocatorWrapper;

#[alloc_error_handler]
pub fn error_handler(_layout: Layout) -> ! {
    idos_api::syscall::exec::terminate(2);
}
