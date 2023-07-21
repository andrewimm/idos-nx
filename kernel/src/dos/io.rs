use crate::task::actions;
use crate::task::files::FileHandle;

use super::error::DosErrorCode;
use super::execution::{get_current_psp_segment, PSP};
use super::memory::SegmentedAddress;

pub fn get_file_handle(dos_handle: u8) -> FileHandle {
    // TODO: this may need another layer of indirection,
    // it's not going to work well if a drive has >255 handles open
    FileHandle::new(dos_handle as usize)
}

pub fn write_file(handle_index: u16, write_length: u16, data_segment: u16, data_offset: u16) -> Result<u16, DosErrorCode> {
    crate::kprintln!("WRITE FILE {} {} {:X}:{:X}", handle_index, write_length, data_segment, data_offset);
    let psp_segment = get_current_psp_segment();
    let psp = unsafe { PSP::at_segment(psp_segment) };
    if handle_index as usize > psp.file_handles.len() {
        return Err(DosErrorCode::InvalidHandle);
    }
    let raw_handle = psp.file_handles[handle_index as usize];
    if raw_handle == 0xff {
        return Err(DosErrorCode::InvalidHandle);
    }
    let handle = get_file_handle(raw_handle);
    let data_address = SegmentedAddress { segment: data_segment, offset: data_offset };
    let data_ptr = data_address.normalize().as_ptr::<u8>();
    let data_slice = unsafe { core::slice::from_raw_parts(data_ptr, write_length as usize) };

    let bytes_written = actions::io::write_file(handle, data_slice).map_err(|_| DosErrorCode::AccessDenied)?;

    Ok(bytes_written as u16)
}
