use core::mem::ManuallyDrop;
use super::super::address::PhysicalAddress;

/// An AllocatedFrame is returned from global methods that allocate physical
/// memory. It ensures that the result is either mapped into memory or freed.
#[must_use]
pub struct AllocatedFrame {
    frame_start: PhysicalAddress,
}

impl AllocatedFrame {
    pub fn new(frame_start: PhysicalAddress) -> Self {
        Self {
            frame_start,
        }
    }

    pub fn to_physical_address(self) -> PhysicalAddress {
        let addr = ManuallyDrop::new(self);
        addr.frame_start
    }
}

impl Drop for AllocatedFrame {
    fn drop(&mut self) {
        panic!("Allocated physical frame must be used or freed");
    }
}
