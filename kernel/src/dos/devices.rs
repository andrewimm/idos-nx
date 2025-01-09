use super::vm::{DosApiRegisters, VM86Frame};

/// AH=0x01 - Read from STDIN and echo to STDOUT
/// Input:
///     None
/// Output:
///     AL = character from STDIN
pub fn read_stdin_with_echo(_regs: &mut DosApiRegisters) {}

/// AH=0x02 - Output single character to STDOUT
/// Input:
///     DL = character to output
/// Output:
///     None
pub fn output_char_to_stdout(_regs: &mut DosApiRegisters) {}

/// AH=0x03 - Blocking character read from STDAUX (COM)
/// Input:
///     None
/// Output:
///     AL = character from STDAUX
pub fn read_stdaux(_regs: &mut DosApiRegisters) {}

/// AH=0x04 - Write character to STDAUX
/// Input:
///     DL = character to output
/// Output:
///     None
pub fn write_stdaux(_regs: &mut DosApiRegisters) {}

/// AH=0x09 - Print a dollar-terminated string to STDOUT
/// Input:
///     DS:DX points to the string
/// Output:
///     None
pub fn print_string(_regs: &mut DosApiRegisters, _segments: &mut VM86Frame) {}
