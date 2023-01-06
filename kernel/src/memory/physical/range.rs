use super::super::address::PhysicalAddress;

pub struct FrameRange {
    start: PhysicalAddress,
    length: u32,
}

impl FrameRange {
    pub fn new(start: PhysicalAddress, length: u32) -> Self {
        Self {
            start,
            length,
        }
    }
}
