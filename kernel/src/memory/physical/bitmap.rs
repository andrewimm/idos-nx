use super::super::address::PhysicalAddress;
use super::bios::{self, MapEntry};
use super::range::FrameRange;

#[derive(PartialEq)]
pub enum BitmapError {
    /// Unable to perform the requested allocation, because a suitable region
    /// of free space was not found
    NoAvailableSpace,
    /// Attempted to allocate or access a frame beyond the physical memory
    /// installed in the system
    OutOfBounds,
}

impl core::fmt::Debug for BitmapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            &BitmapError::NoAvailableSpace => f.write_str("FrameBitmap: No available space"),
            &BitmapError::OutOfBounds => f.write_str("FrameBitmap: Out of bounds"),
        }
    }
}

/// A Frame Bitmap is used to track all physical RAM that is available for use.
/// Each bit represents one 4KiB frame of memory. If the bit is cleared to zero
/// that frame can be allocated by the kernel. As memory is claimed by the
/// system, ranges will have their bits set. When frames are freed, the bits
/// will be cleared again for re-use.
pub struct FrameBitmap {
    /// A frame bitmap simply points to a slice of bytes in memory, which is
    /// used to store allocation information
    map: &'static mut [u8],
}

impl FrameBitmap {
    /// Creates an empty, invalid Frame Bitmap
    pub const fn empty() -> Self {
        Self { map: &mut [] }
    }

    /// Initialize a Frame Bitmap at a specific location in memory.
    /// This assumes paging is disabled, and Physical Addresses can be used
    /// directly.
    pub fn at_location(start: PhysicalAddress, frame_count: usize) -> FrameBitmap {
        // each byte in the map will store data on 8 frames
        let mut byte_size = frame_count >> 3;
        if frame_count & 7 != 0 {
            byte_size += 1;
        }
        let start_addr: u32 = start.into();
        let first_byte_ptr = start_addr as *mut u8;
        FrameBitmap {
            map: unsafe { core::slice::from_raw_parts_mut(first_byte_ptr, byte_size) },
        }
    }

    /// The map was originally initialized relative to a physical address.
    /// Once paging is enabled, this reference needs to be moved to the upper
    /// portion of memory with the rest of the kernel.
    pub fn move_to_highmem(&mut self) {
        let location = (self.map.as_ptr() as usize) | 0xc0000000;
        let size = self.map.len();
        self.map = unsafe { core::slice::from_raw_parts_mut(location as *mut u8, size) };
    }

    /// Reset the entire table to being allocated. This is the first step when
    /// initializing a new table. It is safest to assume nothing is free, and
    /// then clear out the areas marked as free by the BIOS memory map.
    /// This also simplifies the logic for checking the last byte of the
    /// bitmap. The last bits of the final entry will be seen as unavailable.
    pub fn reset(&mut self) {
        for i in 0..self.map.len() {
            self.map[i] = 0xff;
        }
    }

    /// Using a BIOS memory map, de-allocate all ranges marked as free. If this
    /// succeeds, the bitmap will accurately reflect all available areas of
    /// memory, and the method will return the number of free bytes.
    pub fn initialize_from_memory_map(&mut self, map: &[MapEntry]) -> Result<u32, BitmapError> {
        self.reset();
        let mut total_length: u32 = 0;
        for entry in map.iter() {
            if entry.region_type == bios::REGION_TYPE_FREE {
                let length = entry.length as u32;
                let range = FrameRange::new(PhysicalAddress::new(entry.base as u32), length);
                self.free_range(range)?;
                total_length += length
            }
        }
        Ok(total_length)
    }

    /// Mark a range as unused. Any subset of it may be used to fill an
    /// allocation request in the future.
    pub fn free_range(&mut self, range: FrameRange) -> Result<(), BitmapError> {
        if !self.contains_range(range) {
            return Err(BitmapError::OutOfBounds);
        }
        let first = range.get_first_frame_index();
        let last = range.get_last_frame_index();
        for frame in first..=last {
            let byte_index = frame >> 3;
            self.map[byte_index] &= !(1 << (frame & 7));
        }
        Ok(())
    }

    /// How big is this table, in 4096-byte frames? Useful for allocating
    /// itself during initialization.
    pub fn size_in_frames(&self) -> usize {
        let byte_size = self.map.len();
        let frame_count = byte_size >> 12;
        // Round up if necessary
        if byte_size & 0xfff == 0 {
            frame_count
        } else {
            frame_count + 1
        }
    }

    pub fn size_in_bytes(&self) -> usize {
        self.map.len()
    }

    pub fn total_frame_count(&self) -> usize {
        self.map.len() * 8
    }

    /// Compute the number of unallocated frames, showing how much memory is
    /// available.
    pub fn get_free_frame_count(&self) -> usize {
        let mut frame = 0;
        let mut free = 0;
        let total_count = self.total_frame_count();
        while frame < total_count {
            let index = frame >> 3;
            let map_value = self.map[index];
            // TODO: Optimize this with a lookup table
            if map_value != 0xff {
                let mut mask = 1;
                while mask != 0 {
                    if map_value & mask == 0 {
                        free += 1;
                    }
                    mask <<= 1;
                    frame += 1;
                }
            } else {
                frame += 1;
            }
        }

        free
    }

    /// Determines whether an entire range of frames is valid
    pub fn contains_range(&self, range: FrameRange) -> bool {
        let last_frame = range.get_last_frame_index();
        let frame_count = self.total_frame_count();
        last_frame < frame_count
    }

    /// Determines whether a range of frames is entirely unallocated
    pub fn is_range_free(&self, range: FrameRange) -> bool {
        if !self.contains_range(range) {
            return false;
        }
        let first = range.get_first_frame_index();
        let last = range.get_last_frame_index();
        for frame in first..=last {
            let byte_index = frame >> 3;
            let bitmap_byte = self.map[byte_index];
            let byte_offset = frame & 7;
            if bitmap_byte & (1 << byte_offset) != 0 {
                return false;
            }
        }
        true
    }

    /// Finds the first free range containing the requested number of
    /// consecutive frames. If no suitable range is found, returns None.
    pub fn find_free_range(&self, frame_count: usize) -> Option<FrameRange> {
        let mut frame = 0;
        let mut remaining = frame_count;
        let mut search_start = 0;
        let search_end = self.total_frame_count();
        while frame < search_end {
            let byte_index = frame >> 3;
            let frame_mask = 1 << (frame & 7);
            if self.map[byte_index] & frame_mask != 0 {
                // occupied, start the search over
                remaining = frame_count;
                search_start = frame + 1;
            } else {
                remaining -= 1;
                if remaining == 0 {
                    let starting_address = (search_start << 12) as u32;
                    let length = ((frame + 1 - search_start) << 12) as u32;
                    return Some(FrameRange::new(
                        PhysicalAddress::new(starting_address),
                        length,
                    ));
                }
            }

            frame += 1;
        }
        None
    }

    /// Mark a specific range as allocated
    pub fn allocate_range(&mut self, range: FrameRange) -> Result<(), BitmapError> {
        if !self.contains_range(range) {
            return Err(BitmapError::OutOfBounds);
        }
        let first = range.get_first_frame_index();
        let last = range.get_last_frame_index();
        for frame in first..=last {
            let byte_index = frame >> 3;
            self.map[byte_index] |= 1 << (frame & 7);
        }
        Ok(())
    }

    /// Allocate a *physically contiguous* set of frames, returning a reference
    /// to the available memory area.
    /// If you don't need a contiguous block of memory, it may be better to
    /// request one frame at a time.
    pub fn allocate_frames(&mut self, frame_count: usize) -> Result<FrameRange, BitmapError> {
        let range = self
            .find_free_range(frame_count)
            .ok_or(BitmapError::NoAvailableSpace)?;
        self.allocate_range(range).map(|_| range)
    }
}

#[cfg(test)]
mod tests {
    use super::{BitmapError, FrameBitmap, FrameRange, PhysicalAddress};

    #[test_case]
    fn bitmap_creation() {
        let memory: [u8; 4] = [0; 4];
        let bitmap =
            FrameBitmap::at_location(PhysicalAddress::new(&memory[1] as *const u8 as u32), 10);
        assert!(bitmap.is_range_free(FrameRange::new(PhysicalAddress::new(0), 0xa000)));
        assert!(bitmap.is_range_free(FrameRange::new(PhysicalAddress::new(0x5000), 0x3000)));
        assert!(!bitmap.is_range_free(FrameRange::new(PhysicalAddress::new(0), 0x11000)));
    }

    #[test_case]
    fn bitmap_allocate() {
        let memory: [u8; 2] = [0; 2];
        let mut bitmap =
            FrameBitmap::at_location(PhysicalAddress::new(&memory[0] as *const u8 as u32), 10);
        bitmap
            .allocate_range(FrameRange::new(PhysicalAddress::new(0), 0x2000))
            .unwrap();
        assert_eq!(memory, [3, 0]);
        bitmap
            .allocate_range(FrameRange::new(PhysicalAddress::new(0x6000), 0x3000))
            .unwrap();
        assert_eq!(memory, [0xc3, 1]);
        assert_eq!(
            bitmap.allocate_range(FrameRange::new(PhysicalAddress::new(0x10000), 0x7000)),
            Err(BitmapError::OutOfBounds),
        );
        assert_eq!(memory, [0xc3, 1]);
    }

    #[test_case]
    fn bitmap_free() {
        let memory: [u8; 2] = [0; 2];
        let mut bitmap =
            FrameBitmap::at_location(PhysicalAddress::new(&memory[0] as *const u8 as u32), 10);
        bitmap
            .allocate_range(FrameRange::new(PhysicalAddress::new(0), 0xa000))
            .unwrap();
        assert_eq!(memory, [0xff, 0x03]);
        bitmap
            .free_range(FrameRange::new(PhysicalAddress::new(0), 0x3000))
            .unwrap();
        assert_eq!(memory, [0xf8, 0x03]);
        bitmap
            .free_range(FrameRange::new(PhysicalAddress::new(0x8000), 0x2000))
            .unwrap();
        assert_eq!(memory, [0xf8, 0x00]);
    }

    #[test_case]
    fn find_free_range() {
        let memory: [u8; 8] = [0; 8];
        let mut bitmap =
            FrameBitmap::at_location(PhysicalAddress::new(&memory[0] as *const u8 as u32), 60);
        assert_eq!(
            bitmap.find_free_range(4),
            Some(FrameRange::new(PhysicalAddress::new(0), 0x4000)),
        );
        assert_eq!(bitmap.find_free_range(80), None,);
        bitmap
            .allocate_range(FrameRange::new(PhysicalAddress::new(0), 0x2000))
            .unwrap();
        bitmap
            .allocate_range(FrameRange::new(PhysicalAddress::new(0x4000), 0x3000))
            .unwrap();
        assert_eq!(
            bitmap.find_free_range(3),
            Some(FrameRange::new(PhysicalAddress::new(0x7000), 0x3000)),
        );
        assert_eq!(
            bitmap.find_free_range(1),
            Some(FrameRange::new(PhysicalAddress::new(0x2000), 0x1000)),
        );
        bitmap
            .allocate_range(FrameRange::new(PhysicalAddress::new(0x7000), 0xb000))
            .unwrap();
        assert_eq!(
            bitmap.find_free_range(4),
            Some(FrameRange::new(PhysicalAddress::new(0x12000), 0x4000)),
        );
    }

    #[test_case]
    fn free_frame_count() {
        let memory: [u8; 8] = [0; 8];
        let mut bitmap =
            FrameBitmap::at_location(PhysicalAddress::new(&memory[0] as *const u8 as u32), 60);
        bitmap.reset();
        bitmap
            .free_range(FrameRange::new(PhysicalAddress::new(0), 0x3c000))
            .unwrap();
        assert_eq!(bitmap.get_free_frame_count(), 60);
        bitmap.allocate_frames(2).unwrap();
        assert_eq!(bitmap.get_free_frame_count(), 58);
        let range = bitmap.allocate_frames(10).unwrap();
        assert_eq!(bitmap.get_free_frame_count(), 48);
        bitmap.allocate_frames(5).unwrap();
        assert_eq!(bitmap.get_free_frame_count(), 43);
        bitmap.free_range(range).unwrap();
        assert_eq!(bitmap.get_free_frame_count(), 53);
    }
}
