use super::super::address::PhysicalAddress;

#[derive(Copy, Clone, Eq)]
pub struct FrameRange {
    start: PhysicalAddress,
    length: u32,
}

impl FrameRange {
    pub const fn new(start: PhysicalAddress, length: u32) -> Self {
        Self {
            start,
            length,
        }
    }

    pub fn get_first_frame_index(&self) -> usize {
        let start_addr: u32 = self.start.into();
        (start_addr as usize) >> 12
    }

    pub fn get_last_frame_index(&self) -> usize {
        let last_addr = u32::from(self.start) + self.length - 1;
        (last_addr as usize) >> 12
    }

    pub fn get_starting_address(&self) -> PhysicalAddress {
        self.start
    }

    pub fn get_final_address(&self) -> PhysicalAddress {
        self.start + (self.length - 1)
    }

    pub fn contains_address(&self, addr: PhysicalAddress) -> bool {
        let first = self.start;
        let last = self.get_final_address();
        first <= addr && addr <= last
    }

    pub fn size_in_bytes(&self) -> u32 {
        self.length
    }
}

impl PartialEq for FrameRange {
    fn eq(&self, other: &Self) -> bool {
        self.get_starting_address() == other.get_starting_address() &&
        self.size_in_bytes() == other.size_in_bytes()
    }
}

impl core::fmt::Debug for FrameRange {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("FrameRange")
            .field(&self.get_starting_address())
            .field(&self.get_final_address())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameRange, PhysicalAddress};

    #[test_case]
    fn bounds() {
        let f = FrameRange::new(PhysicalAddress::new(0x4000), 0x4000);
        assert_eq!(f.get_first_frame_index(), 4);
        assert_eq!(f.get_last_frame_index(), 7);
        assert_eq!(f.get_starting_address(), PhysicalAddress::new(0x4000));
        assert_eq!(f.get_final_address(), PhysicalAddress::new(0x7fff));
        assert!(f.contains_address(PhysicalAddress::new(0x4000)));
        assert!(f.contains_address(PhysicalAddress::new(0x5005)));
        assert!(f.contains_address(PhysicalAddress::new(0x7fff)));
        assert!(!f.contains_address(PhysicalAddress::new(0x3025)));
        assert!(!f.contains_address(PhysicalAddress::new(0x8000)));
    }
}

