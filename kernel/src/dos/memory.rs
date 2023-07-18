use crate::memory::address::VirtualAddress;

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct SegmentedAddress {
    pub segment: u16,
    pub offset: u16,
}

impl SegmentedAddress {
    pub fn normalize(&self) -> VirtualAddress {
        VirtualAddress::new(
            ((self.segment as u32) << 4) + (self.offset as u32)
        )
    }
}
