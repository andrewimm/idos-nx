use super::memory::SegmentedAddress;

// AH=0 - Terminate program
// Restores the interrupt vectors 0x22, 0x23, 0x24. Frees memory allocated to
// the current program, but does not close FCBs.
// Input:
//      CS points to the PSP
pub fn terminate(cs: u16) -> SegmentedAddress {
    
    crate::task::actions::lifecycle::terminate(0);
}
