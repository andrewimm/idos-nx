use crate::memory::address::{PhysicalAddress, VirtualAddress};
use alloc::{collections::BTreeMap, vec::Vec};
use core::ops::Range;

/// MemMappedRegion represents a section of memory that has been mapped to a
/// Task.
#[derive(Copy, Clone)]
pub struct MemMappedRegion {
    pub address: VirtualAddress,
    pub size: u32,
    pub backed_by: MemoryBacking,
}

impl MemMappedRegion {
    pub fn get_address_range(&self) -> Range<VirtualAddress> {
        let start = self.address;
        let end = start + self.size;
        start..end
    }

    pub fn contains_address(&self, addr: &VirtualAddress) -> bool {
        self.get_address_range().contains(addr)
    }

    pub fn page_count(&self) -> usize {
        let mut count = self.size as usize / 0x1000;
        if self.size & 0xfff != 0 {
            count += 1;
        }
        count
    }
}

/// The backing type of a mem-mapped region determines how it behaves when a
/// page fault occurs. It tells the kernel how to find the memory or data that
/// this page contains
#[derive(Copy, Clone)]
pub enum MemoryBacking {
    /// This region should directly point to a same-sized region of physical
    /// memory. This is necessary for interfacing with devices on the memory
    /// bus.
    Direct(PhysicalAddress),
    /// This region is backed by an arbitrary section of physical memory,
    /// allocated on demand. Continuity is not guaranteed.
    Anonymous,
    /// Similar to Anonymous, but guarantees the memory will be a contiguous
    /// region within the first 16 MiB of physical address space.
    DMA,
}

/// MappedMemory is a collection of memory mappings. The const parameter
/// represents the upper bound of the memory mapped region.
pub struct MappedMemory<const U: u32> {
    regions: BTreeMap<VirtualAddress, MemMappedRegion>,
}

impl<const U: u32> MappedMemory<U> {
    pub const fn new() -> Self {
        Self {
            regions: BTreeMap::new(),
        }
    }

    /// Create a memory mapping. This does not actually modify the page table,
    /// but the next time a page fault occurs in this region the kernel will be
    /// able to use this information to fill in the page.
    /// If a virtual address is provided, the algorithm will attempt to find the
    /// closest available space that is large enough to fit the requested size.
    /// Otherwise, it iterates downwards from the top of the memory mapping
    /// area.
    /// On success, it returns the address that the region has been mapped to.
    pub fn map_memory(
        &mut self,
        addr: Option<VirtualAddress>,
        requested_size: u32,
        backing: MemoryBacking,
    ) -> Result<VirtualAddress, MemMapError> {
        if requested_size == 0 {
            return Err(MemMapError::InvalidSize);
        }
        let size = (requested_size + 0xfff) & 0xfffff000;

        let location: Option<VirtualAddress> = match addr {
            Some(request_start) => {
                if !request_start.is_page_aligned() {
                    return Err(MemMapError::MappingWrongAlignment);
                }
                let request_end = request_start + size;
                if self.can_fit_range(request_start..request_end) {
                    Some(request_start)
                } else {
                    self.find_free_mapping_space(size)
                }
            }
            None => self.find_free_mapping_space(size),
        };

        let free_space = location.ok_or(MemMapError::NotEnoughMemory)?;

        let mapping = MemMappedRegion {
            address: free_space,
            size,
            backed_by: backing,
        };
        self.regions.insert(free_space, mapping);
        Ok(free_space)
    }

    pub fn unmap_memory(
        &mut self,
        addr: VirtualAddress,
        length: u32,
    ) -> Result<Range<VirtualAddress>, MemMapError> {
        if length & 0xfff != 0 {
            return Err(MemMapError::UnmapNotPageMultiple);
        }
        if addr.as_u32() >= U as u32 || addr.as_u32() + length > U as u32 {
            return Err(MemMapError::MapOutOfBounds);
        }

        // Is there a more efficient data structure?
        // Something like an "interval tree" which contains non-overlapping
        // ranges.

        // Iterate over all regions, and find the ones that need to be modified.
        // Once that set has been computed, all intersected regions will be
        // removed from the map, and any remaining sub-regions will be put back.
        let mut unmap_start = addr;
        let mut unmap_length = length;
        let mut modified_regions: Vec<(VirtualAddress, u32, u32)> = Vec::new();
        for (_, region) in self.regions.iter() {
            let region_range = region.get_address_range();
            if unmap_start < region_range.start && (unmap_start + unmap_length) > region_range.start
            {
                let delta = region_range.start - unmap_start;
                unmap_length -= delta;
                unmap_start = unmap_start + delta;
            }
            if region_range.contains(&unmap_start) {
                let can_unmap = region_range.end - unmap_start;
                let (to_remove, remaining) = if can_unmap > unmap_length {
                    (unmap_length, 0)
                } else {
                    (can_unmap, unmap_length - can_unmap)
                };
                unmap_length = remaining;
                let remove_start = unmap_start - region_range.start;
                let remove_end = remove_start + to_remove;
                modified_regions.push((region_range.start, remove_start, remove_end));
                unmap_start = unmap_start + to_remove;
                if remaining == 0 {
                    break;
                }
            }
        }

        for (modification_key, range_start, range_end) in modified_regions {
            let region = self
                .regions
                .remove(&modification_key)
                .expect("Attempted to unmap region that is not mapped");
            if range_start > 0 {
                let before = MemMappedRegion {
                    address: region.address,
                    size: range_start,
                    backed_by: region.backed_by,
                };
                self.regions.insert(region.address, before);
            }
            if range_end < region.size {
                let new_size = region.size - range_end;
                let new_address = region.address + (region.size - new_size);
                let after = MemMappedRegion {
                    address: new_address,
                    size: new_size,
                    backed_by: region.backed_by,
                };
                self.regions.insert(new_address, after);
            }
        }
        Ok(addr..(addr + length))
    }

    /// Returns a reference to a mmap region if it contains the requested
    /// virtual address. This is useful for handling a page fault.
    pub fn get_mapping_containing_address(
        &self,
        addr: &VirtualAddress,
    ) -> Option<&MemMappedRegion> {
        for (_, region) in self.regions.iter() {
            if region.contains_address(addr) {
                return Some(region);
            }
        }
        None
    }

    /// Same as get_mapping_containing_address, but returns a mutable reference
    /// to the region if it is found.
    pub fn get_mut_mapping_containing_address(
        &mut self,
        addr: &VirtualAddress,
    ) -> Option<&mut MemMappedRegion> {
        for (_, region) in self.regions.iter_mut() {
            if region.contains_address(addr) {
                return Some(region);
            }
        }
        None
    }

    /// Checks if the specified range can fit without overlapping any currently
    /// mapped regions.
    fn can_fit_range(&self, range: Range<VirtualAddress>) -> bool {
        // Check for intersection with each mmap'd range
        for (_, mapping) in self.regions.iter() {
            if ranges_overlap(&mapping.get_address_range(), &range) {
                return false;
            }
        }

        true
    }

    /// Finds a free mapping space for the requested size.
    /// Iterate backwards through the mapped set. If the space between the
    /// current region and the previous one is large enough to fit the requested
    /// size, return an address in that space.
    pub fn find_free_mapping_space(&self, size: u32) -> Option<VirtualAddress> {
        // Start at the top of the memory space and work downwards
        let mut prev_start = VirtualAddress::new(U);
        // The mapped regions are sorted by address, and they don't overlap, so
        // a reverse iterator over all values will visit them in descending
        // order.
        // prev_start represents the last visited mapped region, and is the
        // top of any possible allocation area.
        for (_, region) in self.regions.iter().rev() {
            let region_end = (region.address + region.size).next_page_barrier();
            let free_space = prev_start - region_end;
            if free_space >= size {
                let addr = (prev_start - size).prev_page_barrier();
                return Some(addr);
            }
            prev_start = region.address;
        }

        Some((prev_start - size).prev_page_barrier())
    }
}

pub fn ranges_overlap(a: &Range<VirtualAddress>, b: &Range<VirtualAddress>) -> bool {
    let min = a.start.min(b.start);
    let max = a.end.max(b.end);
    let a_length = a.end - a.start;
    let b_length = b.end - b.start;
    (a_length + b_length) > (max - min)
}

#[derive(Debug)]
pub enum MemMapError {
    /// Invalid size for memory mapping (must be > 0)
    InvalidSize,
    /// Attempted to map a region of memory beyond the highest possible location
    MapOutOfBounds,
    /// Attempted to map a region of memory that is not page-aligned
    MappingWrongAlignment,
    /// Not enough free virtual memory to satisfy the mapping request
    NotEnoughMemory,
    /// Unmap requests must be a multiple of the page size
    UnmapNotPageMultiple,
    /// Attempted to map memory for a task that does not exist
    NoTask,
    /// Attempted to remap a region that was not mapped
    NotMapped,
    /// Some error occurred while mapping memory for a task
    MappingFailed,
}

#[cfg(test)]
mod tests {
    use super::{ranges_overlap, MappedMemory, MemoryBacking, VirtualAddress};

    #[test_case]
    fn overlapping_ranges() {
        assert!(!ranges_overlap(
            &(VirtualAddress::new(0x100)..VirtualAddress::new(0x200)),
            &(VirtualAddress::new(0x300)..VirtualAddress::new(0x400)),
        ));

        assert!(!ranges_overlap(
            &(VirtualAddress::new(0x100)..VirtualAddress::new(0x200)),
            &(VirtualAddress::new(0x200)..VirtualAddress::new(0x400)),
        ));

        assert!(ranges_overlap(
            &(VirtualAddress::new(0x100)..VirtualAddress::new(0x200)),
            &(VirtualAddress::new(0x1ff)..VirtualAddress::new(0x400)),
        ));

        assert!(ranges_overlap(
            &(VirtualAddress::new(0x100)..VirtualAddress::new(0x300)),
            &(VirtualAddress::new(0x200)..VirtualAddress::new(0x220)),
        ));

        assert!(ranges_overlap(
            &(VirtualAddress::new(0x100)..VirtualAddress::new(0x300)),
            &(VirtualAddress::new(0x200)..VirtualAddress::new(0x400)),
        ));
    }

    #[test_case]
    fn explicit_mmap() {
        let mut regions = MappedMemory::new();
        assert_eq!(
            regions
                .map_memory(
                    Some(VirtualAddress::new(0x4000)),
                    0x1000,
                    MemoryBacking::Anonymous
                )
                .unwrap(),
            VirtualAddress::new(0x4000),
        );
        assert_eq!(
            regions
                .map_memory(
                    Some(VirtualAddress::new(0x6000)),
                    0x2000,
                    MemoryBacking::Anonymous
                )
                .unwrap(),
            VirtualAddress::new(0x6000),
        );
        assert_eq!(
            regions
                .map_memory(
                    Some(VirtualAddress::new(0x5000)),
                    0x2000,
                    MemoryBacking::Anonymous
                )
                .unwrap(),
            VirtualAddress::new(0xbfffc000),
        );
    }

    #[test_case]
    fn auto_allocated_mmap() {
        let mut regions = MappedMemory::new();
        assert_eq!(
            regions
                .map_memory(None, 0x1000, MemoryBacking::Anonymous)
                .unwrap(),
            VirtualAddress::new(0xbfffd000),
        );
        assert_eq!(
            regions
                .map_memory(None, 0x400, MemoryBacking::Anonymous)
                .unwrap(),
            VirtualAddress::new(0xbfffc000),
        );
    }

    #[test_case]
    fn unmapping() {
        let mut regions = MappedMemory::new();
        regions
            .map_memory(
                Some(VirtualAddress::new(0x1000)),
                0x1000,
                MemoryBacking::Anonymous,
            )
            .unwrap();
        assert_eq!(
            regions
                .unmap_memory(VirtualAddress::new(0x1000), 0x1000)
                .unwrap(),
            VirtualAddress::new(0x1000)..VirtualAddress::new(0x2000),
        );
        assert!(regions.mapped_regions.is_empty());

        regions
            .map_memory(
                Some(VirtualAddress::new(0x1000)),
                0x2000,
                MemoryBacking::Anonymous,
            )
            .unwrap();
        regions
            .map_memory(
                Some(VirtualAddress::new(0x4000)),
                0x3000,
                MemoryBacking::Anonymous,
            )
            .unwrap();
        assert_eq!(
            regions
                .unmap_memory(VirtualAddress::new(0x2000), 0x2000)
                .unwrap(),
            VirtualAddress::new(0x2000)..VirtualAddress::new(0x4000),
        );

        {
            let shrunk = regions
                .mapped_regions
                .get(&VirtualAddress::new(0x1000))
                .unwrap();
            assert_eq!(shrunk.address, VirtualAddress::new(0x1000));
            assert_eq!(shrunk.size, 0x1000);
        }

        assert_eq!(regions.mapped_regions.len(), 2);
        assert_eq!(
            regions
                .unmap_memory(VirtualAddress::new(0x1000), 0x4000)
                .unwrap(),
            VirtualAddress::new(0x1000)..VirtualAddress::new(0x5000),
        );

        {
            let shrunk = regions
                .mapped_regions
                .get(&VirtualAddress::new(0x5000))
                .unwrap();
            assert_eq!(shrunk.address, VirtualAddress::new(0x5000));
            assert_eq!(shrunk.size, 0x2000);
        }
        assert_eq!(regions.mapped_regions.len(), 1);
    }
}
