use crate::interrupts::stack::StackFrame;

use super::{execution, devices};
use super::vm::{DosApiRegisters, VM86Frame};

pub fn dos_api(regs: &mut DosApiRegisters, segments: &mut VM86Frame, stack_frame: &StackFrame) {
    match regs.ah() {
        0x00 => { // Terminate
            let new_address = execution::terminate(stack_frame.cs as u16);
            // if this code is still executing, it was running a DOS program
            // launched by another DOS program, all within the same Task/VM
            stack_frame.set_cs(new_address.segment as u32);
            stack_frame.set_eip(new_address.offset as u32);
        },
        0x01 => { // Keyboard input with Echo
        },
        0x02 => { // Print character to STDOUT
        },
        0x03 => { // Wait for STDAUX
        },
        0x04 => { // Output to STDAUX
        },
        0x05 => { // Output to Printer
        },
        0x06 => { // Console IO methods
        },
        0x07 => { // Blocking console keyboard input
        },
        0x08 => { // Blocking STDIN input
        },
        0x09 => { // Print string
            devices::print_string(regs, segments);
        },

        _ => {},
    }
}
