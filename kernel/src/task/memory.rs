use crate::{
    io::filesystem::driver::DriverID,
    memory::address::{PhysicalAddress, VirtualAddress},
};
use alloc::{collections::BTreeMap, vec::Vec};
use core::ops::Range;
use idos_api::io::driver::DriverMappingToken;

/// MemMappedRegion represents a section of memory that has been mapped to a
/// Task.
#[derive(Clone)]
pub struct MemMappedRegion {
    /// The starting virtual address of the mapped region
    pub address: VirtualAddress,
    /// The size of the mapped region, in bytes. This is not guaranteed to be a
    /// multiple of page size. Any bytes beyond this length in the final page
    /// are unpredictable (but really, they'll probably just be zero)
    pub size: u32,
    /// The backing type of this memory region, used to determine how to handle
    /// page faults.
    pub backed_by: MemoryBacking,
}

impl MemMappedRegion {
    pub fn get_address_range(&self) -> Range<VirtualAddress> {
        let start = self.address;
        let end = start + self.size;
        start..end
    }

    /// Checks if the given virtual address is contained within this memory
    /// region. If an address is in the final page, but beyond the size of the
    /// region, this will return false, which probably triggers a page fault
    /// later on.
    pub fn contains_address(&self, addr: &VirtualAddress) -> bool {
        self.get_address_range().contains(addr)
    }

    pub fn page_count(&self) -> usize {
        (self.size as usize + 0xfff) / 0x1000
    }
}

/// The backing type of a mem-mapped region determines how it behaves when a
/// page fault occurs. It tells the kernel how to find the memory or data that
/// this page contains
#[derive(Clone)]
pub enum MemoryBacking {
    /// This region should directly point to a same-sized region of physical
    /// memory. This is necessary for interfacing with devices on the memory
    /// bus.
    Direct(PhysicalAddress),
    /// This region is backed by an arbitrary section of physical memory,
    /// allocated on demand. Continuity is not guaranteed.
    FreeMemory,
    /// Similar to FreeMemory, but guarantees the memory will be a contiguous
    /// region within the first 16 MiB of physical address space.
    IsaDma,
    /// This region is backed by a file on disk. The kernel will read data from
    /// the file as needed when page faults occur.
    FileBacked {
        /// The driver providing the file data
        driver_id: DriverID,
        /// A token provided by the driver mapping
        mapping_token: DriverMappingToken,
        /// The offset within the file where this mapping starts
        offset_in_file: u32,
    },
}

pub struct UnmappedRegion {
    pub address: VirtualAddress,
    pub size: u32,
    pub kind: UnmappedRegionKind,
}

pub enum UnmappedRegionKind {
    Direct(PhysicalAddress),
    FreeMemory,
    FileBacked {
        driver_id: DriverID,
        mapping_token: DriverMappingToken,
        offset_in_file: u32,
    },
}

/// MappedMemory is a collection of memory mappings. The const parameter
/// represents the upper bound of the memory mapped region.
/// This struct is stored on every Task. It doesn't directly modify any page
/// tables, but it tells the kernel how to handle a page fault. When a page
/// fault exception occurs, the kernel looks at the current Task's MappedMemory
/// to determine whether the faulting address is valid and how to handle it.
pub struct MappedMemory<const U: u32> {
    regions: BTreeMap<VirtualAddress, MemMappedRegion>,
}

impl<const U: u32> MappedMemory<U> {
    pub const fn new() -> Self {
        Self {
            regions: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.regions.len()
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
        let rounded_size = (requested_size + 0xfff) & 0xfffff000;

        let location: Option<VirtualAddress> = match addr {
            Some(request_start) => {
                if !request_start.is_page_aligned() {
                    return Err(MemMapError::MappingWrongAlignment);
                }
                let request_end = request_start + rounded_size;
                if self.can_fit_range(request_start..request_end) {
                    Some(request_start)
                } else {
                    self.find_free_mapping_space(rounded_size)
                }
            }
            None => self.find_free_mapping_space(rounded_size),
        };

        let free_space = location.ok_or(MemMapError::NotEnoughMemory)?;

        let mapping = MemMappedRegion {
            address: free_space,
            size: requested_size,
            backed_by: backing,
        };
        self.regions.insert(free_space, mapping);
        Ok(free_space)
    }

    /// Remove a Task's reference to a region of mapped memory. This does not
    /// directly modify any page table -- that is handled separately.
    /// The requested unmapped region may overlap with one or more current
    /// mappings. Those intersections are removed, leaving behind any
    /// non-intersecting parts as new mappings.
    /// The method returns a `Vec` of `UnmappedRegion`s, which represent the
    /// sections that were unmapped and can now be safely cleaned up by other
    /// memory management systems.
    pub fn unmap_memory(
        &mut self,
        addr: VirtualAddress,
        length: u32,
    ) -> Result<Vec<UnmappedRegion>, MemMapError> {
        if length & 0xfff != 0 {
            return Err(MemMapError::UnmapNotPageMultiple);
        }
        if addr.as_u32() >= U as u32 || addr.as_u32() + length > U as u32 {
            return Err(MemMapError::MapOutOfBounds);
        }

        // An interval tree could be more efficient, but it's extra overhead
        // for the relatively low number of mappings we expect each task to
        // have.

        let mut unmapped: Vec<UnmappedRegion> = Vec::new();
        let unmap_end = addr + length;
        // The first mapping region that intersects with our query is either the
        // last one before our address, or the first one after:
        let initial_key = if let Some((k, _)) = self.regions.range(..=addr).next_back() {
            *k
        } else if let Some((k, _)) = self.regions.range(addr..).next() {
            *k
        } else {
            // No mappings at all, so we can just return
            return Ok(unmapped);
        };

        let mut key_to_modify: Vec<VirtualAddress> = Vec::new();
        for (k, v) in self.regions.range(initial_key..) {
            if v.get_address_range().start >= unmap_end {
                break;
            }
            if v.get_address_range().end <= addr {
                continue;
            }
            key_to_modify.push(*k);
        }

        for k in key_to_modify {
            let region = self
                .regions
                .remove(&k)
                .expect("Attempted to remove mmap region that is not mapped");
            let region_range = region.get_address_range();
            let intersection_start = region_range.start.max(addr);
            let intersection_end = region_range.end.min(unmap_end);
            assert!(intersection_end >= intersection_start);
            let intersection_size = intersection_end - intersection_start;
            let local_offset = intersection_start - region_range.start;
            if intersection_size > 0 {
                unmapped.push(UnmappedRegion {
                    address: intersection_start,
                    size: intersection_size,
                    kind: match region.backed_by {
                        MemoryBacking::Direct(paddr) => {
                            UnmappedRegionKind::Direct(paddr + local_offset)
                        }
                        MemoryBacking::FreeMemory => UnmappedRegionKind::FreeMemory,
                        MemoryBacking::IsaDma => UnmappedRegionKind::FreeMemory,
                        MemoryBacking::FileBacked {
                            driver_id,
                            mapping_token,
                            offset_in_file,
                        } => UnmappedRegionKind::FileBacked {
                            driver_id,
                            mapping_token,
                            offset_in_file: offset_in_file + local_offset,
                        },
                    },
                });
            }
            if region_range.start < addr {
                let before = MemMappedRegion {
                    address: region_range.start,
                    size: addr - region_range.start,
                    backed_by: region.backed_by.clone(),
                };
                self.regions.insert(before.address, before);
            }
            if region_range.end > unmap_end {
                let after = MemMappedRegion {
                    address: unmap_end,
                    size: region_range.end - unmap_end,
                    backed_by: region.backed_by.clone(),
                };
                self.regions.insert(after.address, after);
            }
        }

        Ok(unmapped)
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
    /// File-backed memory mapping failed because the driver or file could not be found
    FileUnavailable,
    /// An error occurred in the backing driver while mapping memory
    DriverError,
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
        let mut regions = MappedMemory::<0xbfff_e000>::new();
        assert_eq!(
            regions
                .map_memory(
                    Some(VirtualAddress::new(0x4000)),
                    0x1000,
                    MemoryBacking::FreeMemory
                )
                .unwrap(),
            VirtualAddress::new(0x4000),
        );
        assert_eq!(
            regions
                .map_memory(
                    Some(VirtualAddress::new(0x6000)),
                    0x2000,
                    MemoryBacking::FreeMemory
                )
                .unwrap(),
            VirtualAddress::new(0x6000),
        );
        assert_eq!(
            regions
                .map_memory(
                    Some(VirtualAddress::new(0x5000)),
                    0x2000,
                    MemoryBacking::FreeMemory
                )
                .unwrap(),
            VirtualAddress::new(0xbfffc000),
        );
    }

    #[test_case]
    fn auto_allocated_mmap() {
        let mut regions = MappedMemory::<0xbfff_e000>::new();
        assert_eq!(
            regions
                .map_memory(None, 0x1000, MemoryBacking::FreeMemory)
                .unwrap(),
            VirtualAddress::new(0xbfffd000),
        );
        assert_eq!(
            regions
                .map_memory(None, 0x400, MemoryBacking::FreeMemory)
                .unwrap(),
            VirtualAddress::new(0xbfffc000),
        );
    }

    // TODO: unmap tests
}
