use crate::interrupts::stack::StackFrame;
use crate::memory::address::VirtualAddress;
use crate::memory::physical::allocate_frame;
use crate::task::paging::{current_pagedir_map, page_on_demand, PermissionFlags};

use super::error::DosErrorCode;
use super::execution::{get_current_psp_segment, PSP};

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct SegmentedAddress {
    pub offset: u16,
    pub segment: u16,
}

impl SegmentedAddress {
    pub fn normalize(&self) -> VirtualAddress {
        VirtualAddress::new(((self.segment as u32) << 4) + (self.offset as u32))
    }
}

pub fn handle_page_fault(_stack_frame: &StackFrame, address: u32) -> bool {
    let vaddr = VirtualAddress::new(address);
    let page_start = vaddr.prev_page_barrier();
    if !page_on_demand(vaddr).is_none() {
        // this was part of the memory map
        let current_segment = get_current_psp_segment();
        if (page_start.as_u32() >> 4) as u16 == current_segment {
            // It was the page with the PSP
            crate::kprintln!("WRITE PSP");
            let psp = unsafe { PSP::at_segment(current_segment) };
            psp.reset();
        }

        return true;
    } else {
        // other handling for memory to emulate the real mode environment
        crate::kprintln!("DOS ACCESS MEMORY: {:X}", address);
        if page_start.as_u32() == 0 {
            // first page is full of PC/DOS internals

            let allocated_frame = allocate_frame().unwrap();
            let flags =
                PermissionFlags::new(PermissionFlags::USER_ACCESS | PermissionFlags::WRITE_ACCESS);
            let _paddr = current_pagedir_map(allocated_frame, page_start, flags);
            return true;
        }

        // idk just allocate free memory for now
        let allocated_frame = allocate_frame().unwrap();
        let flags =
            PermissionFlags::new(PermissionFlags::USER_ACCESS | PermissionFlags::WRITE_ACCESS);
        current_pagedir_map(allocated_frame, page_start, flags);
        return true;
    }
}

/// AH=0x48 - Allocate a block of memory
/// Creates a new MCB after the current one. If successful, returns the segment
/// of that memory block. If not enough space is available, an error will be
/// returned indicating how much space is free.
/// Input:
///     BX = paragraphs to request
/// Output (Success):
///     AX = segment of new memory block
///     CF clear
/// Output (Error):
///     AX = error code
///     BX = number of paragraphs available
///     CF set
pub fn allocate_mcb(_paragraphs_requested: u16) -> Result<u16, (DosErrorCode, u16)> {
    crate::kprintln!("!!! DOS API unimplemented 0x48");
    Ok(0x200)
}

/// AH=0x49 - Free a block of memory
pub fn free_mcb(_mcb_segment: u16) -> Result<(), DosErrorCode> {
    crate::kprintln!("!!! DOS API unimplemented 0x49");
    Ok(())
}

/// AH=0x4a - Resize a block of memory
pub fn resize_mcb(
    _mcb_segment: u16,
    _paragraphs_requested: u16,
) -> Result<(), (DosErrorCode, u16)> {
    crate::kprintln!("!!! DOS API unimplemented 0x4a");
    Ok(())
}
