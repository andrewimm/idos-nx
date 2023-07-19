use crate::interrupts::stack::StackFrame;
use crate::memory::address::VirtualAddress;
use crate::task::paging::page_on_demand;

use super::execution::{get_current_psp_segment, PSP};

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

pub fn handle_page_fault(stack_frame: &StackFrame, address: u32) -> bool {
    let vaddr = VirtualAddress::new(address);
    if !page_on_demand(vaddr).is_none() {
        // this was part of the memory map
        let current_segment = get_current_psp_segment();
        if (vaddr.prev_page_barrier().as_u32() >> 4) as u16 == current_segment {
            // It was the page with the PSP
            let psp = unsafe { PSP::at_segment(current_segment) };
            psp.reset();
        }

        return true;
    }
    // other handling for memory to emulate the real mode environment

    false
}
