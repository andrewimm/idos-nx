use crate::task::actions::io::write_file;
use crate::task::files::FileHandle;

use super::memory::SegmentedAddress;
use super::vm::{DosApiRegisters, VM86Frame};


/// AH=0x09 - Print a dollar-terminated string to STDOUT
/// Input:
///     DS:DX points to the string
/// Output:
///     None
pub fn print_string(regs: &mut DosApiRegisters, segments: &mut VM86Frame) {
    // TODO: this needs to be the PSP's STDOUT, not the Task's...
    let stdout_handle = FileHandle::new(1);
    
    let string_location = SegmentedAddress {
        segment: segments.ds as u16,
        offset: regs.dx as u16,
    };
    let start = string_location.normalize().as_ptr::<u8>();
    let mut length = 0;
    loop {
        if length > 255 {
            break;
        }
        let ch = unsafe { *start.add(length) };
        if ch == b'$' {
            break;
        }
        length += 1;
    }
    if length == 0 {
        return;
    }

    let buffer = unsafe { core::slice::from_raw_parts(start, length) };
    write_file(stdout_handle, buffer);
}
