//! DOS API calls related to devices

use idos_api::compat::VMRegisters;

/// AH=0x01 - Read from STDIN and echo to STDOUT
/// Input:
///     None
/// Output:
///     AL = character from STDIN
pub fn read_stdin_with_echo(regs: &mut VMRegisters) {
    let stdin = idos_api::io::handle::Handle::new(0);
    let stdout = idos_api::io::handle::Handle::new(1);

    let mut buffer: [u8; 1] = [0; 1];

    match idos_api::io::sync::read_sync(stdin, &mut buffer, 0) {
        Ok(len) if len == 1 => {
            let _ = idos_api::io::sync::write_sync(stdout, &mut buffer, 0);
        }
        _ => (),
    }

    regs.set_al(buffer[0]);
}

/// AH=0x02 - Output single character to STDOUT
/// Input:
///     DL = character to output
/// Output:
///     None
pub fn output_char_to_stdout(regs: &mut VMRegisters) {
    let char = regs.dl();
    let buffer: [u8; 1] = [char];
    let stdout = idos_api::io::handle::Handle::new(1);
    let _ = idos_api::io::sync::write_sync(stdout, &buffer, 0);
}

/// AH=0x03 - Blocking character read from STDAUX (COM)
/// Input:
///     None
/// Output:
///     AL = character from STDAUX
pub fn read_char_stdaux(_regs: &mut VMRegisters) {}

/// AH=0x04 - Write character to STDAUX
/// Input:
///     DL = character to output
/// Output:
///     None
pub fn write_char_stdaux(regs: &mut VMRegisters) {
    let char = regs.dl();
    let buffer: [u8; 1] = [char];
    let stdaux = idos_api::io::handle::Handle::new(2);
    let _ = idos_api::io::sync::write_sync(stdaux, &buffer, 0);
}

/// AH=0x09 - Print a dollar-terminated string to STDOUT
/// Input:
///     DS:DX points to the string
/// Output:
///     None
pub fn print_string(regs: &mut VMRegisters) {
    let dx = regs.edx & 0xffff;
    let start_address = (regs.ds << 4) + dx;
    let start_ptr = start_address as *const u8;
    let search_len = 256.min(0x10000 - dx) as usize;
    let mut string_len = 0;
    while string_len < search_len {
        unsafe {
            if core::ptr::read_volatile(start_ptr.add(string_len)) == b'$' {
                break;
            }
        }
        string_len += 1;
    }
    let string_slice = unsafe { core::slice::from_raw_parts(start_ptr, string_len) };
    let stdout = idos_api::io::handle::Handle::new(1);
    let _ = idos_api::io::sync::write_sync(stdout, string_slice, 0);
}
