use crate::io::handle::FileHandle;

pub fn write_raw(handle: FileHandle, ptr: *const u8, len: usize) -> usize {
    super::syscall(0x13, *handle, ptr as u32, len as u32) as usize
}

pub fn write_str(handle: FileHandle, s: &str) -> usize {
    write_raw(handle, s.as_ptr(), s.len())
}
