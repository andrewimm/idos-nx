pub mod allocated_frame;
pub mod bios;
pub mod bitmap;
pub mod range;

use allocated_frame::AllocatedFrame;
use bios::load_memory_map;
use bitmap::{BitmapError, FrameBitmap};
use range::FrameRange;
use spin::Mutex;
use super::address::PhysicalAddress;

static mut ALLOCATOR: Mutex<FrameBitmap> = Mutex::new(FrameBitmap::empty());

pub const FRAME_SIZE: u32 = 0x1000;

pub fn init_allocator(location: PhysicalAddress, memory_map_address: PhysicalAddress, kernel_range: FrameRange) {
    // Get the memory map from BIOS to know how much memory is installed
    let memory_map = load_memory_map(memory_map_address);
    let mut memory_end = 0;
    // The memory map from BIOS is not guaranteed to be in order
    crate::kprint!("System Memory Map:\n");
    for entry in memory_map.iter() {
        let start = entry.base as usize;
        let end = start + (entry.length as usize);
        if end > memory_end {
            memory_end = end;
        }
        crate::kprint!("{:?}\n", entry);
    }

    let mut bitmap = FrameBitmap::at_location(location, memory_end >> 12);
    bitmap.initialize_from_memory_map(memory_map).unwrap();

    // Mark the frame bitmap itself as allocated, so that it won't be reused
    let size_in_frames = bitmap.size_in_frames() as u32;
    let own_range = FrameRange::new(location, size_in_frames as u32 * FRAME_SIZE);
    bitmap.allocate_range(own_range).unwrap();
    // Mark the kernel segments as allocated
    bitmap.allocate_range(kernel_range).unwrap();
    // Mark the first 0x1000 bytes as occupied, too. We may need the BIOS data
    bitmap.allocate_range(FrameRange::new(PhysicalAddress::new(0), 0x1000)).unwrap();

    crate::kprint!(
        "Total Memory: {} KiB\nFree Memory: {} KiB\n",
        bitmap.total_frame_count() * 4,
        bitmap.get_free_frame_count() * 4,
    );

    unsafe {
        ALLOCATOR = Mutex::new(bitmap);
    }
}

pub fn with_allocator<F, T>(f: F) -> T where
    F: Fn(&mut FrameBitmap) -> T {
    // Safe because the ALLOCATOR will only be set once, synchronously
    let mut alloc = unsafe { ALLOCATOR.lock() };
    f(&mut alloc)
}

pub fn allocate_frame() -> Result<AllocatedFrame, BitmapError> {
    let frame_address = with_allocator(|alloc| {
        alloc
            .allocate_frames(1)
            .map(|range| range.get_starting_address())
    });
    frame_address.map(|addr| AllocatedFrame::new(addr))
}

pub fn release_frame(address: PhysicalAddress) -> Result<(), BitmapError> {
    let range = FrameRange::new(address, FRAME_SIZE);
    with_allocator(|alloc| {
        alloc.free_range(range)
    })
}

pub fn get_allocator_size() -> usize {
    with_allocator(|alloc| {
        alloc.size_in_bytes()
    })
}

