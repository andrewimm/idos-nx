pub mod bios;
pub mod bitmap;
pub mod range;

use bios::load_memory_map;
use bitmap::FrameBitmap;
use super::address::PhysicalAddress;

pub fn init_allocator(location: PhysicalAddress, memory_map_address: PhysicalAddress) {
    // Get the memory map from BIOS to know how much memory is installed
    let memory_map = load_memory_map(memory_map_address);
    let mut memory_end = 0;
    // The memory map from BIOS is not guaranteed to be in order
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
}


