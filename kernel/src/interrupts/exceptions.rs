use crate::asm;
use crate::memory::address::VirtualAddress;
use crate::task::actions::lifecycle::exception;
use crate::task::paging::page_on_demand;
use crate::task::switching::get_current_id;

use super::stack::StackFrame;

/// Triggered when dividing by zero, or when the result is too large to fit in
/// the destination register.
#[no_mangle]
pub extern "x86-interrupt" fn div(_stack_frame: StackFrame) {
    // send a soft interrupt to the current task indicating an arithmetic exception
    crate::kprint!("Divide by zero\n");
    exception();
}

/// Debug trap used for a number of tracing modes like single-step
#[no_mangle]
pub extern "x86-interrupt" fn debug(_stack_frame: StackFrame) {
    panic!("Debug trap");
}

#[no_mangle]
pub extern "x86-interrupt" fn nmi(_stack_frame: StackFrame) {
    panic!("NMI");
}

/// Triggered by the INT 3 instruction. Used to stop execution and alert a
/// debugger, if one is attached.
#[no_mangle]
pub extern "x86-interrupt" fn breakpoint(_stack_frame: StackFrame) {
    let _current_lock = crate::task::switching::get_current_task();
    // look for task that might be tracing this one

    panic!("Break");
}

#[no_mangle]
pub extern "x86-interrupt" fn overflow(_stack_frame: StackFrame) {
    panic!("Overflow");
}

#[no_mangle]
pub extern "x86-interrupt" fn bound_exceeded(_stack_frame: StackFrame) {
    panic!("BOUND Range Exceeded");
}

#[no_mangle]
pub extern "x86-interrupt" fn invalid_opcode(_stack_frame: StackFrame) {
    panic!("Invalid Opcode");
}

#[no_mangle]
pub extern "x86-interrupt" fn fpu_not_available(_stack_frame: StackFrame) {
    panic!("FPU not available");
}

#[no_mangle]
pub extern "x86-interrupt" fn double_fault(_stack_frame: StackFrame, _error: u32) {
    loop {}
}

#[no_mangle]
pub extern "x86-interrupt" fn invalid_tss(_stack_frame: StackFrame, _error: u32) {
    loop {}
}

#[no_mangle]
pub extern "x86-interrupt" fn segment_not_present(_stack_frame: StackFrame, _error: u32) {
    loop {}
}

#[no_mangle]
pub extern "x86-interrupt" fn stack_segment_fault(_stack_frame: StackFrame, _error: u32) {
    loop {}
}

#[no_mangle]
pub extern "x86-interrupt" fn gpf(stack_frame: StackFrame, error: u32) {
    if stack_frame.eflags & 0x20000 != 0 {
        // VM86 Mode
        if crate::dos::vm::handle_gpf(&stack_frame) {
            return;
        }
    } else if stack_frame.eip >= 0xc0000000 {
        crate::kprintln!("Kernel GPF: {}", error);
        loop {}
    }

    crate::kprintln!("ERR: General Protection Fault, code {}", error);
    crate::kprintln!("{:?}", stack_frame);
    crate::task::actions::lifecycle::terminate(0);
}

#[no_mangle]
pub extern "x86-interrupt" fn page_fault(stack_frame: StackFrame, error: u32) {
    let address: u32;
    unsafe {
        asm!(
            "mov {0:e}, cr2",
            out(reg) address,
        );
    }
    let eip = stack_frame.eip;
    let cur_id = get_current_id();
    crate::kprint!("\nPage Fault ({:?}: {:#010X}) at {:#010X} ({:X})\n", cur_id, eip, address, error);

    if address >= 0xc0000000 { // Kernel region
        if error & 4 == 4 {
            // Permission error - access attempt did not come from ring 0
            // This should segfault
            crate::kprintln!("User program attempted to reach out-of-bounds memory");
            crate::task::actions::lifecycle::terminate(0);
        }
        if error & 1 == 0 {
            // Page was not present
            crate::kprint!("Attempted to reach unpaged kernel memory. Does heap need to be expanded?");
            loop {}
        }
    } else { // User space
        if stack_frame.eflags & 0x20000 != 0 {
            // handle VM86 page faults separately
            if crate::dos::memory::handle_page_fault(&stack_frame, address) {
                return;
            }
        }

        if error & 1 == 0 {
            // Page was not present
            // Let the current task determine how to handle the missing page
            let vaddr = VirtualAddress::new(address);
            if !page_on_demand(vaddr).is_none() {
                // Return back to the failed memory access
                return;
            }
        } else if error & 2 == 2 {
            // Write to a read-only page
            crate::kprint!("Write to page {:?}", cur_id);
        }

        
        // All other cases (accessing an unmapped section, writing a read-only
        // segment, etc) should cause a segfault.
        crate::kprint!("SEGFAULT AT IP: {:#010X} (Access {:#010X})\n", eip, address);
    }
    crate::task::actions::lifecycle::terminate(0);
}

