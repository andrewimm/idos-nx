//! Directory reading via kernel file I/O.
//!
//! The kernel treats directories as readable files where entries
//! are null-byte separated filenames.

use core::ffi::{c_char, c_int};
use core::ptr;
use core::sync::atomic::Ordering;

use idos_api::io::{AsyncOp, Handle, ASYNC_OP_CLOSE, ASYNC_OP_OPEN, ASYNC_OP_READ};
use idos_api::syscall::exec::futex_wait_u32;
use idos_api::syscall::io::{append_io_op, create_file_handle};

const DIR_BUF_SIZE: usize = 512;
const MAX_OPEN_DIRS: usize = 8;

pub struct DIR {
    handle: Handle,
    /// Read buffer
    buf: [u8; DIR_BUF_SIZE],
    /// Number of valid bytes in buf
    buf_len: usize,
    /// Current position within buf
    buf_pos: usize,
    /// File read offset
    read_offset: u32,
    /// Reached end of directory
    eof: bool,
    /// Is this slot in use
    in_use: bool,
}

#[repr(C)]
pub struct dirent {
    pub d_name: [c_char; 256],
}

static mut DIR_TABLE: [DIR; MAX_OPEN_DIRS] = unsafe { core::mem::zeroed() };
static mut DIRENT_BUF: dirent = dirent { d_name: [0; 256] };

fn io_sync_raw(handle: Handle, op_code: u32, arg0: u32, arg1: u32, arg2: u32) -> Result<u32, u32> {
    let op = AsyncOp::new(op_code, arg0, arg1, arg2);
    append_io_op(handle, &op, None);
    while op.signal.load(Ordering::SeqCst) == 0 {
        futex_wait_u32(&op.signal, 0, None);
    }
    let ret = op.return_value.load(Ordering::SeqCst);
    if ret & 0x80000000 != 0 {
        Err(ret & 0x7fffffff)
    } else {
        Ok(ret)
    }
}

#[no_mangle]
pub unsafe extern "C" fn opendir(name: *const c_char) -> *mut DIR {
    if name.is_null() {
        return ptr::null_mut();
    }

    // Find free slot
    let mut slot: Option<usize> = None;
    for i in 0..MAX_OPEN_DIRS {
        if !DIR_TABLE[i].in_use {
            slot = Some(i);
            break;
        }
    }
    let idx = match slot {
        Some(i) => i,
        None => return ptr::null_mut(),
    };

    // Translate path
    let mut path_buf = [0u8; 256];
    let path_len = crate::stdio::translate_path_raw(name, &mut path_buf);

    let handle = create_file_handle();
    let result = io_sync_raw(
        handle,
        ASYNC_OP_OPEN,
        path_buf.as_ptr() as u32,
        path_len as u32,
        0,
    );

    if result.is_err() {
        io_sync_raw(handle, ASYNC_OP_CLOSE, 0, 0, 0).ok();
        return ptr::null_mut();
    }

    DIR_TABLE[idx] = DIR {
        handle,
        buf: [0; DIR_BUF_SIZE],
        buf_len: 0,
        buf_pos: 0,
        read_offset: 0,
        eof: false,
        in_use: true,
    };

    &raw mut DIR_TABLE[idx]
}

#[no_mangle]
pub unsafe extern "C" fn readdir(dirp: *mut DIR) -> *mut dirent {
    if dirp.is_null() || !(*dirp).in_use {
        return ptr::null_mut();
    }

    loop {
        // Try to find a null-terminated entry in the current buffer
        let start = (*dirp).buf_pos;
        for i in start..(*dirp).buf_len {
            if (*dirp).buf[i] == 0 {
                // Found entry from start..i
                let name_len = i - start;
                if name_len == 0 {
                    (*dirp).buf_pos = i + 1;
                    continue;
                }
                // Copy to dirent
                for j in 0..name_len {
                    DIRENT_BUF.d_name[j] = (*dirp).buf[start + j] as c_char;
                }
                DIRENT_BUF.d_name[name_len] = 0;
                (*dirp).buf_pos = i + 1;
                return &raw mut DIRENT_BUF;
            }
        }

        // No complete entry found; need to read more
        if (*dirp).eof {
            return ptr::null_mut();
        }

        // Move any partial entry to beginning of buffer
        let remaining = (*dirp).buf_len - (*dirp).buf_pos;
        if remaining > 0 && (*dirp).buf_pos > 0 {
            ptr::copy(
                (*dirp).buf.as_ptr().add((*dirp).buf_pos),
                (*dirp).buf.as_mut_ptr(),
                remaining,
            );
        }
        (*dirp).buf_len = remaining;
        (*dirp).buf_pos = 0;

        // Read more data
        let read_buf = &mut (*dirp).buf[remaining..];
        let result = io_sync_raw(
            (*dirp).handle,
            ASYNC_OP_READ,
            read_buf.as_ptr() as u32,
            read_buf.len() as u32,
            (*dirp).read_offset,
        );

        match result {
            Ok(n) if n > 0 => {
                (*dirp).buf_len += n as usize;
                (*dirp).read_offset += n;
            }
            _ => {
                (*dirp).eof = true;
                if remaining == 0 {
                    return ptr::null_mut();
                }
                // Process any remaining partial entry
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn closedir(dirp: *mut DIR) -> c_int {
    if dirp.is_null() || !(*dirp).in_use {
        return -1;
    }
    io_sync_raw((*dirp).handle, ASYNC_OP_CLOSE, 0, 0, 0).ok();
    (*dirp).in_use = false;
    0
}
