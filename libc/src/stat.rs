//! POSIX stat backed by kernel async I/O.

use core::ffi::{c_char, c_int};

use idos_api::io::file::FileStatus;
use idos_api::io::{ASYNC_OP_CLOSE, ASYNC_OP_OPEN, FILE_OP_STAT};
use idos_api::syscall::io::create_file_handle;

use crate::stdio::{io_sync, translate_path_raw};

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

/// POSIX file type bits
const S_IFREG: u32 = 0o100000;
const S_IFDIR: u32 = 0o040000;

#[no_mangle]
pub unsafe extern "C" fn stat(pathname: *const c_char, statbuf: *mut Stat) -> c_int {
    if pathname.is_null() || statbuf.is_null() {
        return -1;
    }

    let handle = create_file_handle();

    // Translate the path for the kernel
    let mut path_buf = [0u8; 256];
    let path_len = translate_path_raw(pathname, &mut path_buf);

    // Open the file
    let open_result = io_sync(
        handle,
        ASYNC_OP_OPEN,
        path_buf.as_ptr() as u32,
        path_len as u32,
        0,
    );

    if open_result.is_err() {
        io_sync(handle, ASYNC_OP_CLOSE, 0, 0, 0).ok();
        return -1;
    }

    // Issue stat
    let mut fs = FileStatus::new();
    let stat_result = io_sync(
        handle,
        FILE_OP_STAT,
        &mut fs as *mut FileStatus as u32,
        core::mem::size_of::<FileStatus>() as u32,
        0,
    );

    // Close the handle
    io_sync(handle, ASYNC_OP_CLOSE, 0, 0, 0).ok();

    if stat_result.is_err() {
        return -1;
    }

    // Map kernel FileStatus to POSIX struct stat
    let mode = if fs.file_type == 2 {
        S_IFDIR | 0o755
    } else {
        S_IFREG | 0o644
    };

    core::ptr::write_bytes(statbuf as *mut u8, 0, core::mem::size_of::<Stat>());
    (*statbuf).st_dev = fs.drive_id;
    (*statbuf).st_mode = mode;
    (*statbuf).st_nlink = 1;
    (*statbuf).st_size = fs.byte_size as i32;
    (*statbuf).st_blksize = 512;
    (*statbuf).st_blocks = ((fs.byte_size + 511) / 512) as u32;
    (*statbuf).st_mtime = fs.modification_time as i32;
    (*statbuf).st_atime = fs.modification_time as i32;
    (*statbuf).st_ctime = fs.modification_time as i32;

    0
}

#[no_mangle]
pub unsafe extern "C" fn fstat(_fd: c_int, statbuf: *mut Stat) -> c_int {
    // fstat on an open fd would need access to the kernel handle, which
    // we don't track by raw fd today. Zero-fill for now.
    if !statbuf.is_null() {
        core::ptr::write_bytes(statbuf as *mut u8, 0, core::mem::size_of::<Stat>());
        (*statbuf).st_mode = S_IFREG | 0o644;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn mkdir(_pathname: *const c_char, _mode: u32) -> c_int {
    0 // stub success
}

#[no_mangle]
pub unsafe extern "C" fn chmod(_path: *const c_char, _mode: u32) -> c_int {
    0 // stub success
}
