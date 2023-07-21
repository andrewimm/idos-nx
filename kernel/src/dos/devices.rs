use crate::task::actions::io::{write_file, read_file};
use crate::task::files::FileHandle;

use super::execution::{get_current_psp_segment, PSP};
use super::memory::SegmentedAddress;
use super::vm::{DosApiRegisters, VM86Frame};

/// AH=0x01 - Read from STDIN and echo to STDOUT
/// Input:
///     None
/// Output:
///     AL = character from STDIN
pub fn read_stdin_with_echo(regs: &mut DosApiRegisters) {
    let mut buffer: [u8; 1] = [0];
    let psp = unsafe { PSP::at_segment(get_current_psp_segment()) };
    let stdin_handle = FileHandle::new(psp.file_handles[0] as usize);
    let stdout_handle = FileHandle::new(psp.file_handles[1] as usize);

    let len = match read_file(stdin_handle, &mut buffer) {
        Ok(len) => len,
        Err(_) => return,
    };

    if len > 0 {
        regs.set_al(buffer[0]);
        let _ = write_file(stdout_handle, &buffer);
    }
}

/// AH=0x02 - Output single character to STDOUT
/// Input:
///     DL = character to output
/// Output:
///     None
pub fn output_char_to_stdout(regs: &mut DosApiRegisters) {
    let psp = unsafe { PSP::at_segment(get_current_psp_segment()) };
    let stdout_handle = FileHandle::new(psp.file_handles[1] as usize);
    let buffer: [u8; 1] = [regs.dl()];
    let _ = write_file(stdout_handle, &buffer);
}

/// AH=0x03 - Blocking character read from STDAUX (COM)
/// Input:
///     None
/// Output:
///     AL = character from STDAUX
pub fn read_stdaux(regs: &mut DosApiRegisters) {
    let psp = unsafe { PSP::at_segment(get_current_psp_segment()) };
    let stdaux_handle = FileHandle::new(psp.file_handles[3] as usize);
    let mut buffer: [u8; 1] = [0];

    let len = match read_file(stdaux_handle, &mut buffer) {
        Ok(len) => len,
        Err(_) => return,
    };

    if len > 0 {
        regs.set_al(buffer[0]);
    }
}

/// AH=0x04 - Write character to STDAUX
/// Input:
///     DL = character to output
/// Output:
///     None
pub fn write_stdaux(regs: &mut DosApiRegisters) {
    let psp = unsafe { PSP::at_segment(get_current_psp_segment()) };
    let stdaux_handle = FileHandle::new(psp.file_handles[3] as usize);
    let buffer: [u8; 1] = [regs.dl()];
    let _ = write_file(stdaux_handle, &buffer);
}

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
        offset: regs.dx(),
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
