use alloc::{collections::BTreeMap, vec::Vec};
use core::ops::Range;
use crate::{memory::address::{VirtualAddress, PhysicalAddress}, filesystem::get_driver_by_id};

use super::files::OpenFile;

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

/// When executing a program, the kernel builds a mapping between sections of
/// the executable file and locations where they should appear in virtual
/// memory.
/// Formats like ELF formally define these mappings. The kernel is designed to
/// be flexible enough to support interpretation of multiple execution formats.
/// An ExecutionSection is core to this mapping. When an area of virtual memory
/// needs to be initialized, this mapping determines how to load the bytes from
/// the source.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionSection {
    /// Where in the parent segment does this section get loaded to?
    pub segment_offset: u32,
    /// Where is this section found in the executable file. If None, no data
    /// needs to be loaded from the file, and the memory will be zeroed out
    pub executable_file_offset: Option<u32>,
    /// Section size, in bytes
    pub size: u32,
}

impl ExecutionSection {
    /// Constructs a `Range` of addresses that are covered by this section.
    /// This is useful for determining if this section contains a specific
    /// address
    pub fn address_range(&self, segment_start: VirtualAddress) -> Range<VirtualAddress> {
        let start = segment_start + self.segment_offset;
        let end = start + self.size;
        start..end
    }

    /// True if the segment contains no addresses
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Trim the start or end of a section to fit within a specific range of
    /// segment offsets. This is used to determine how much of a section fits
    /// within a page of memory
    pub fn clip_to(&self, range: Range<u32>) -> Option<ExecutionSection> {
        if self.segment_offset >= range.end {
            return None;
        }
        if self.segment_offset + self.size < range.start {
            return None;
        }
        let (start, offset) = if self.segment_offset < range.start {
            let delta = range.start - self.segment_offset;
            (range.start, self.executable_file_offset.map(|off| off + delta))
        } else {
            (self.segment_offset, self.executable_file_offset)
        };
        let end = range.end.min(self.segment_offset + self.size);
        let size = if end > start { end - start } else { 0 };
        Some(
            ExecutionSection {
                segment_offset: start,
                executable_file_offset: offset,
                size,
            }
        )
    }
}

/// An ExecutionSegment associates a series of virtual memory pages with data
/// stored in an executable file.
/// Each segment has a single set of read/write permissions, and must be
/// page-aligned. These values determine how the page table entry is
/// constructed.
#[derive(Clone)]
pub struct ExecutionSegment {
    /// Where the segment begins in virtual memory. Must be page-aligned.
    pub address: VirtualAddress,
    /// The size of the segment, in pages
    pub size_in_pages: u32,
    /// The full set of sections found within this segment. Because segments
    /// are rounded up to the next page boundary, not all addresses may map to
    /// a section.
    pub sections: Vec<ExecutionSection>,
    /// Is the section user-writable?
    pub can_write: bool,
}

impl ExecutionSegment {
    /// Construct a new ExecutionSegment that begins at the specified virtual
    /// address, and contains the requested number of pages
    pub fn at_address(address: VirtualAddress, size_in_pages: u32) -> Result<Self, TaskMemoryError> {
        if !address.is_page_aligned() {
            return Err(TaskMemoryError::SegmentWrongAlignment);
        }
        Ok(
            Self {
                address,
                size_in_pages,
                sections: Vec::new(),
                can_write: false,
            }
        )
    }

    pub fn get_starting_address(&self) -> VirtualAddress {
        self.address
    }

    pub fn size_in_bytes(&self) -> u32 {
        self.size_in_pages * 0x1000
    }

    pub fn add_section(&mut self, section: ExecutionSection) -> Result<(), TaskMemoryError> {
        if section.segment_offset + section.size > self.size_in_bytes() {
            return Err(TaskMemoryError::SectionOutOfBounds);
        }
        self.sections.push(section);
        Ok(())
    }

    pub fn set_user_write_flag(&mut self, flag: bool) {
        self.can_write = flag;
    }

    pub fn can_user_write(&self) -> bool {
        self.can_write
    }

    pub fn contains_address(&self, address: &VirtualAddress) -> bool {
        for section in self.sections.iter() {
            if section.address_range(self.address).contains(address) {
                return true;
            }
        }
        false
    }

    pub fn sections_iter(&self) -> impl Iterator<Item = &ExecutionSection> {
        self.sections.iter()
    }

    pub fn fill_frame(&self, open_file: OpenFile, page_start: VirtualAddress) {
        // because the segment is guaranteed to start on a page boundary, if it
        // intersects with this frame it must begin before or at page_start
        let start_offset = page_start.as_u32() - self.address.as_u32();
        let end_offset = start_offset + 0x1000;
        let clipped = self
            .sections_iter()
            .map(|s| s.clip_to(start_offset..end_offset));

        for clipped_section in clipped {
            let section = match clipped_section {
                Some(section) => {
                    if section.is_empty() {
                        continue;
                    }
                    section
                },
                None => continue,
            };

            let write_to = self.get_starting_address() + section.segment_offset;
            let write_len = section.size as usize;

            let mut buffer = unsafe {
                let write_ptr = write_to.as_u32() as *mut u8;
                core::slice::from_raw_parts_mut(write_ptr, write_len)
            };

            match section.executable_file_offset {
                Some(file_offset) => {
                    crate::kprint!("Filling exec memory from \"{}\"\n", open_file.filename.as_str());
                    get_driver_by_id(open_file.drive).unwrap()
                        .read(open_file.driver_handle, &mut buffer).unwrap();
                },
                None => {
                    for i in 0..buffer.len() {
                        buffer[i] = 0;
                    }
                },
            }
        }
    }
}

pub struct TaskMemory {
    /// A series of execution segments representing the program's code and data
    execution_segments: Vec<ExecutionSegment>,
    /// Collection of mem-mapped regions allocated to the task
    mapped_regions: BTreeMap<VirtualAddress, MemMappedRegion>,
}

impl TaskMemory {
    pub fn new() -> Self {
        Self {
            execution_segments: Vec::new(),
            mapped_regions: BTreeMap::new(),
        }
    }

    /// Create a memory mapping for this task. This does not actually modify
    /// the page table, but the next time a page fault occurs in this region
    /// the kernel will be able to use this information to fill in the page.
    /// On success, it returns the address that the region has been mapped to.
    pub fn map_memory(&mut self, addr: Option<VirtualAddress>, size: u32, backing: MemoryBacking) -> Result<VirtualAddress, TaskMemoryError> {
        // Find an appropriate spot in virtual memory. If the caller specified
        // a location, we want to find the closest available space; otherwise,
        // crawl through the existing allocations until an appropriately sized
        // space is found.
        let location: Option<VirtualAddress> = match addr {
            Some(request_start) => {
                if !request_start.is_page_aligned() {
                    return Err(TaskMemoryError::MappingWrongAlignment);
                }
                let request_end = request_start + size;
                if self.can_fit_range(request_start..request_end) {
                    Some(request_start)
                } else {
                    self.find_free_mapping_space(size)
                }
            },
            None => self.find_free_mapping_space(size),
        };

        let free_space = location.ok_or(TaskMemoryError::NotEnoughMemory)?;

        let mapping = MemMappedRegion {
            address: free_space,
            size,
            backed_by: backing,
        };
        self.mapped_regions.insert(free_space, mapping);
        Ok(free_space)
    }

    pub fn unmap_memory(&mut self, addr: VirtualAddress, length: u32) -> Result<Range<VirtualAddress>, TaskMemoryError> {
        if length & 0xfff != 0 {
            return Err(TaskMemoryError::UnmapNotPageMultiple);
        }
        if addr.as_u32() >= 0xc0000000u32 || addr.as_u32() + length >= 0xc0000000 {
            return Err(TaskMemoryError::MapOutOfBounds);
        }

        // Hmm... should really replace this BTree with an Interval Tree.
        // Iterate over all regions, and find the ones that need to be modified
        // Once that set has been computed, all intersected regions will be
        // removed from the map, and any remaining sub-regions will be put back
        let mut unmap_start = addr;
        let mut unmap_length = length;
        let mut modified_regions: Vec<(VirtualAddress, Range<u32>)> = Vec::new();
        for (_, region) in self.mapped_regions.iter() {
            let region_range = region.get_address_range();
            if unmap_start < region_range.start && (unmap_start + unmap_length) > region_range.start {
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
                modified_regions.push((region_range.start, remove_start..remove_end));
                unmap_start = unmap_start + to_remove;
                if remaining == 0 {
                    break;
                }
            }
        }
        for modification in modified_regions {
            let region = self.mapped_regions.remove(&modification.0)
                .expect("Attempted to unmap region that is not mapped");
            if modification.1.start > 0 {
                let before = MemMappedRegion {
                    address: region.address,
                    size: modification.1.start,
                    backed_by: region.backed_by,
                };
                self.mapped_regions.insert(region.address, before);
            }
            if modification.1.end < region.size {
                let new_size = region.size - modification.1.end;
                let new_address = region.address + (region.size - new_size);
                let after = MemMappedRegion {
                    address: new_address,
                    size: new_size,
                    backed_by: region.backed_by,
                };
                self.mapped_regions.insert(new_address, after);
            }
        }
        Ok(addr..(addr + length))
    }

    /// Return a reference to a mmap region if it contains the requested
    /// address. This is useful for handling a page fault.
    pub fn get_mapping_containing_address(&self, addr: &VirtualAddress) -> Option<&MemMappedRegion> {
        for (_, region) in self.mapped_regions.iter() {
            if region.contains_address(addr) {
                return Some(region);
            }
        }
        None
    }

    pub fn get_execution_segment_containing_address(&self, addr: &VirtualAddress) -> Option<&ExecutionSegment> {
        for segment in self.execution_segments.iter() {
            if segment.contains_address(addr) {
                return Some(segment);
            }
        }
        None
    }

    fn can_fit_range(&self, range: Range<VirtualAddress>) -> bool {
        // Check for intersection with each mmap'd range
        for (_, mapping) in self.mapped_regions.iter() {
            if ranges_overlap(&mapping.get_address_range(), &range) {
                return false;
            }
        }
        // Check other mappings that get added later (executable segments, etc)

        true
    }

    fn find_free_mapping_space(&self, size: u32) -> Option<VirtualAddress> {
        // Iterate backwards through the mapped set. If the space between the
        // current region and the previous one is large enough to fit the
        // requested size,

        // No memory can be mapped above this point
        let memory_top = 0xc0000000;
        let mut prev_start = VirtualAddress::new(memory_top);
        for (_, region) in self.mapped_regions.iter().rev() {
            let region_end = (region.address + region.size).next_page_barrier();
            let free_space = prev_start - region_end;
            if free_space >= size {
                return Some((prev_start - size).prev_page_barrier());
            }
            prev_start = region.address;
        }
        // TODO: Check this doesn't intersect with other allocated memory
        Some((prev_start - size).prev_page_barrier())
    }

    pub fn has_execution_segments(&self) -> bool {
        !self.execution_segments.is_empty()
    }

    /// Apply the set of execution segments, returning the previously set one
    pub fn set_execution_segments(&mut self, segments: Vec<ExecutionSegment>) -> Vec<ExecutionSegment> {
        core::mem::replace(&mut self.execution_segments, segments)
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
pub enum TaskMemoryError {
    MapOutOfBounds,
    MappingWrongAlignment,
    NoTask,
    NotEnoughMemory,
    NotMapped,
    SegmentWrongAlignment,
    SectionOutOfBounds,
    UnmapNotPageMultiple,
    MappingFailed,
}

#[cfg(test)]
mod tests {
    use super::{ranges_overlap, MemoryBacking, TaskMemory, VirtualAddress};

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
        let mut regions = TaskMemory::new();
        assert_eq!(
            regions.map_memory(Some(VirtualAddress::new(0x4000)), 0x1000, MemoryBacking::Anonymous).unwrap(),
            VirtualAddress::new(0x4000),
        );
        assert_eq!(
            regions.map_memory(Some(VirtualAddress::new(0x6000)), 0x2000, MemoryBacking::Anonymous).unwrap(),
            VirtualAddress::new(0x6000),
        );
        assert_eq!(
            regions.map_memory(Some(VirtualAddress::new(0x5000)), 0x2000, MemoryBacking::Anonymous).unwrap(),
            VirtualAddress::new(0xbfffe000),
        );
    }

    #[test_case]
    fn auto_allocated_mmap() {
        let mut regions = TaskMemory::new();
        assert_eq!(
            regions.map_memory(None, 0x1000, MemoryBacking::Anonymous).unwrap(),
            VirtualAddress::new(0xbffff000),
        );
        assert_eq!(
            regions.map_memory(None, 0x400, MemoryBacking::Anonymous).unwrap(),
            VirtualAddress::new(0xbfffe000),
        );
    }

    #[test_case]
    fn unmapping() {
        let mut regions = TaskMemory::new();
        regions.map_memory(Some(VirtualAddress::new(0x1000)), 0x1000, MemoryBacking::Anonymous).unwrap();
        assert_eq!(
            regions.unmap_memory(VirtualAddress::new(0x1000), 0x1000).unwrap(),
            VirtualAddress::new(0x1000)..VirtualAddress::new(0x2000),
        );
        assert!(regions.mapped_regions.is_empty());

        regions.map_memory(Some(VirtualAddress::new(0x1000)), 0x2000, MemoryBacking::Anonymous).unwrap();
        regions.map_memory(Some(VirtualAddress::new(0x4000)), 0x3000, MemoryBacking::Anonymous).unwrap();
        assert_eq!(
            regions.unmap_memory(VirtualAddress::new(0x2000), 0x2000).unwrap(),
            VirtualAddress::new(0x2000)..VirtualAddress::new(0x4000),
        );

        {
            let shrunk = regions.mapped_regions.get(&VirtualAddress::new(0x1000)).unwrap();
            assert_eq!(shrunk.address, VirtualAddress::new(0x1000));
            assert_eq!(shrunk.size, 0x1000);
        }
    
        assert_eq!(regions.mapped_regions.len(), 2);
        assert_eq!(
            regions.unmap_memory(VirtualAddress::new(0x1000), 0x4000).unwrap(),
            VirtualAddress::new(0x1000)..VirtualAddress::new(0x5000),
        );
    
        {
            let shrunk = regions.mapped_regions.get(&VirtualAddress::new(0x5000)).unwrap();
            assert_eq!(shrunk.address, VirtualAddress::new(0x5000));
            assert_eq!(shrunk.size, 0x2000);
        }
        assert_eq!(regions.mapped_regions.len(), 1);
    }
}

