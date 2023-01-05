pub mod bios;

use bios::load_memory_map;
use super::address::PhysicalAddress;

pub fn init_allocator(location: PhysicalAddress, memory_map_address: PhysicalAddress) {
    // Get the memory map from BIOS to know how much memory is installed
    let memory_map = load_memory_map(memory_map_address);
    let mut limit = 0;
    // The memory map from BIOS is not guaranteed to be in order
    for entry in memory_map.iter() {
        crate::kprint!("{:?}\n", entry);
    }
}
