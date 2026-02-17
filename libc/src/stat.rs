//! stat/fstat/mkdir stubs.

use core::ffi::{c_char, c_int};

#[repr(C)]
pub struct Stat {
    pub st_dev: u32,
    pub st_ino: u32,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u32,
    pub st_size: i32,
    pub st_blksize: u32,
    pub st_blocks: u32,
    pub st_atime: i32,
    pub st_mtime: i32,
    pub st_ctime: i32,
}

#[no_mangle]
pub unsafe extern "C" fn stat(_pathname: *const c_char, statbuf: *mut Stat) -> c_int {
    // Stub: zero out the struct and return success
    if !statbuf.is_null() {
        core::ptr::write_bytes(statbuf as *mut u8, 0, core::mem::size_of::<Stat>());
        // Mark as regular file
        (*statbuf).st_mode = 0o100644;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn fstat(_fd: c_int, statbuf: *mut Stat) -> c_int {
    stat(core::ptr::null(), statbuf)
}

#[no_mangle]
pub unsafe extern "C" fn mkdir(_pathname: *const c_char, _mode: u32) -> c_int {
    0 // stub success
}

#[no_mangle]
pub unsafe extern "C" fn chmod(_path: *const c_char, _mode: u32) -> c_int {
    0 // stub success
}
