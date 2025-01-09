use crate::interrupts::stack::StackFrame;

use super::vm::{DosApiRegisters, VM86Frame};
use super::{devices, execution, memory, system};

pub fn dos_api(regs: &mut DosApiRegisters, segments: &mut VM86Frame, stack_frame: &StackFrame) {
    let eip = stack_frame.eip;
    crate::kprintln!("DOS API CALL AT {:#X}", eip);
    match regs.ah() {
        0x00 => {
            // Terminate
            let new_address = execution::legacy_terminate(stack_frame.cs as u16);
            // if this code is still executing, it was running a DOS program
            // launched by another DOS program, all within the same Task/VM
            stack_frame.set_cs(new_address.segment as u32);
            stack_frame.set_eip(new_address.offset as u32);
        }
        0x01 => {
            // Keyboard input with Echo
            devices::read_stdin_with_echo(regs);
        }
        0x02 => {
            // Print character to STDOUT
            devices::output_char_to_stdout(regs);
        }
        0x03 => { // Wait for STDAUX
        }
        0x04 => { // Output to STDAUX
        }
        0x05 => { // Output to Printer
        }
        0x06 => { // Console IO methods
        }
        0x07 => { // Blocking console keyboard input
        }
        0x08 => { // Blocking STDIN input
        }
        0x09 => {
            // Print string
            devices::print_string(regs, segments);
        }

        0x30 => {
            // Get version
            let (major, minor) = system::get_version();
            regs.set_al(major);
            regs.set_ah(minor);
        }

        0x40 => {
            // Write to file using handle
        }

        0x44 => {
            // IOCTL
            match regs.al() {
                0x00 => {
                    // query device flags
                    // TODO: temporary, say a file is a device
                    regs.set_dx(0x80);
                }
                _ => panic!("Unsupported IOCTL subcommand"),
            }
        }

        0x48 => {
            // Allocate memory blocks
            let paragraphs_requested = regs.bx();
            match memory::allocate_mcb(paragraphs_requested) {
                Ok(segment) => {
                    regs.set_ax(segment);
                    stack_frame.clear_carry_flag();
                }
                Err((code, space_available)) => {
                    regs.set_ax(code as u16);
                    regs.set_bx(space_available);
                    stack_frame.set_carry_flag();
                }
            }
        }
        0x49 => {
            // Free memory blocks
            let mcb_segment = segments.es as u16;
            match memory::free_mcb(mcb_segment) {
                Ok(_) => {
                    stack_frame.clear_carry_flag();
                }
                Err(code) => {
                    regs.set_ax(code as u16);
                    stack_frame.set_carry_flag();
                }
            }
        }

        0x4a => {
            // Modify allocated memory blocks
            let mcb_segment = segments.es as u16;
            let paragraphs_requested = regs.bx();
            match memory::resize_mcb(mcb_segment, paragraphs_requested) {
                Ok(_) => {
                    stack_frame.clear_carry_flag();
                }
                Err((code, space_available)) => {
                    regs.set_ax(code as u16);
                    regs.set_bx(space_available);
                    stack_frame.set_carry_flag();
                }
            }
        }

        0x4c => {
            // Terminate
            let exit_code = regs.al();
            execution::terminate(exit_code);
        }

        0x63 => {
            // Get lead byte table, not supported
            regs.set_al(0xff);
        }

        _ => {
            let eip = stack_frame.eip;
            panic!("Unimplemented DOS API: {:#X}, EIP: {:#X}", regs.ah(), eip);
        }
    }
}
