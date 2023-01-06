use super::range::FrameRange;
use super::bios::{self, MapEntry};
use super::super::address::PhysicalAddress;

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
                let range = FrameRange::new(
                    PhysicalAddress::new(entry.base as u32),
                    length,
                );
                self.free_range(range)?;
                total_length += length
            }
        }
        Ok(total_length)
    }

    pub fn free_range(&mut self, range: FrameRange) -> Result<(), BitmapError> {
        Ok(())
    }
}

