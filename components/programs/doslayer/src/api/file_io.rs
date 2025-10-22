//! FCB and handle-based File IO

use idos_api::compat::VMRegisters;

/// AH=0x0F - Open file with FCB
/// Input:
///     DS:DX = pointer to FCB
/// Output:
///     AL = 0 if successful, 0xFF if error
pub fn open_file_fcb(regs: &mut VMRegisters) {}

/// AH=0x10 - Close file with FCB
/// Input:
///     DS:DX = pointer to FCB
/// Output:
///     AL = 0 if successful, 0xFF if error
pub fn close_file_fcb(regs: &mut VMRegisters) {}

/// AH=0x11 - Search for first file with FCB
/// Input:
///     DS:DX = pointer to FCB
/// Output:
///     AL = 0 if successful, 0xFF if error
pub fn search_first_file_fcb(regs: &mut VMRegisters) {}

/// AH=0x12 - Search for next file with FCB
/// Input:
///     DS:DX = pointer to FCB
/// Output:
///     AL = 0 if successful, 0xFF if error
pub fn search_next_file_fcb(regs: &mut VMRegisters) {}

/// AH=0x13 - Delete file with FCB
/// Input:
///     DS:DX = pointer to FCB
/// Output:
///     AL = 0 if successful, 0xFF if error
pub fn delete_file_fcb(regs: &mut VMRegisters) {}

/// AH=0x14 - Read sequentially from file with FCB
/// Input:
///     DS:DX = pointer to FCB
/// Output:
///     AL = 0x00 if successful,
///          0x01 if end of file (no data read),
///          0x02 if DTA is too small
///          0x03 if end of file or partial read
pub fn read_sequential_fcb(regs: &mut VMRegisters) {}

/// AH=0x15 - Write sequentially to file with FCB
/// Input:
///     DS:DX = pointer to FCB
/// Output:
///     AL = 0x00 if successful,
///          0x01 if disk full or write error
///          0x02 if DTA is too small
pub fn write_sequential_fcb(regs: &mut VMRegisters) {}

/// AH=0x16 - Create file with FCB
/// Input:
///     DS:DX = pointer to FCB
/// Output:
///     AL = 0 if successful, 0xFF if error
pub fn create_file_fcb(regs: &mut VMRegisters) {}

/// AH=0x17 - Rename file with FCB
/// Input:
///     DS:DX = pointer to FCB with custom format
///             Offset 0: Original drive (1 byte)
///             Offset 1: Original file name (8 bytes)
///             Offset 9: Original file extension (3 bytes)
///             Offset 12: New file name (8 bytes)
///             Offset 20: New file extension (3 bytes)
pub fn rename_file_fcb(regs: &mut VMRegisters) {}
