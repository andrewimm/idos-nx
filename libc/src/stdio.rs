//! C stdio implementation backed by kernel async I/O.

use core::ffi::{c_char, c_int, c_void, VaList};
use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering};

use idos_api::io::{AsyncOp, Handle, ASYNC_OP_CLOSE, ASYNC_OP_OPEN, ASYNC_OP_READ, ASYNC_OP_WRITE, FILE_OP_STAT, FILE_OP_IOCTL};
use idos_api::syscall::exec::futex_wait_u32;
use idos_api::syscall::io::{append_io_op, create_file_handle};

// ---- FILE structure ----

const FILE_BUF_SIZE: usize = 1024;

pub const SEEK_SET: c_int = 0;
pub const SEEK_CUR: c_int = 1;
pub const SEEK_END: c_int = 2;

pub const EOF: c_int = -1;

#[repr(C)]
pub struct FILE {
    handle: Handle,
    /// Current file position (for seekable files)
    pos: u32,
    /// Error flag
    error: c_int,
    /// EOF flag
    eof: c_int,
    /// Is this file handle open?
    is_open: bool,
    /// Is this a console (stdin/stdout/stderr)?
    is_console: bool,
    /// Unget buffer (-1 if empty)
    unget: c_int,
}

// Fixed file table
const MAX_FILES: usize = 32;
static mut FILE_TABLE: [FILE; MAX_FILES] = unsafe { core::mem::zeroed() };
static mut FILES_INITIALIZED: bool = false;

/// Get the kernel Handle for a given file descriptor (FILE_TABLE index).
/// Returns None if the fd is out of range or not open.
pub(crate) unsafe fn fd_handle(fd: c_int) -> Option<Handle> {
    if fd < 0 || fd as usize >= MAX_FILES {
        return None;
    }
    let f = &FILE_TABLE[fd as usize];
    if !f.is_open {
        return None;
    }
    Some(f.handle)
}

/// Initialize stdin/stdout/stderr.
/// These use kernel handle indices that the command shell has set up.
pub fn init() {
    unsafe {
        FILES_INITIALIZED = true;
        // stdin = file table entry 0, uses kernel handle for console read
        FILE_TABLE[0] = FILE {
            handle: Handle::new(0),
            pos: 0,
            error: 0,
            eof: 0,
            is_open: true,
            is_console: true,
            unget: -1,
        };
        // stdout = file table entry 1
        FILE_TABLE[1] = FILE {
            handle: Handle::new(1),
            pos: 0,
            error: 0,
            eof: 0,
            is_open: true,
            is_console: true,
            unget: -1,
        };
        // stderr = same as stdout for now
        FILE_TABLE[2] = FILE {
            handle: Handle::new(1),
            pos: 0,
            error: 0,
            eof: 0,
            is_open: true,
            is_console: true,
            unget: -1,
        };
    }
}

// Public pointers to stdin/stdout/stderr
#[no_mangle]
pub static mut stdin: *mut FILE = ptr::null_mut();
#[no_mangle]
pub static mut stdout: *mut FILE = ptr::null_mut();
#[no_mangle]
pub static mut stderr: *mut FILE = ptr::null_mut();

/// Must be called after init() to set up the global pointers.
/// Called from __libc_init.
pub unsafe fn init_std_pointers() {
    stdin = &raw mut FILE_TABLE[0];
    stdout = &raw mut FILE_TABLE[1];
    stderr = &raw mut FILE_TABLE[2];
}

// ---- Internal helpers ----

fn io_sync(handle: Handle, op_code: u32, arg0: u32, arg1: u32, arg2: u32) -> Result<u32, u32> {
    let async_op = AsyncOp::new(op_code, arg0, arg1, arg2);
    append_io_op(handle, &async_op, None);

    while async_op.signal.load(Ordering::SeqCst) == 0 {
        futex_wait_u32(&async_op.signal, 0, None);
    }

    let ret = async_op.return_value.load(Ordering::SeqCst);
    if ret & 0x80000000 != 0 {
        Err(ret & 0x7fffffff)
    } else {
        Ok(ret)
    }
}

/// Translate a C file path for the kernel.
/// Public so dirent.rs can reuse it.
pub unsafe fn translate_path_raw(path: *const c_char, buf: &mut [u8]) -> usize {
    translate_path(path, buf)
}

/// Translate a C file path for the kernel:
/// - Convert '/' to '\'
/// - If path doesn't start with a drive letter, prepend "C:"
unsafe fn translate_path(path: *const c_char, buf: &mut [u8]) -> usize {
    let path_len = crate::string::strlen(path);
    let src = core::slice::from_raw_parts(path as *const u8, path_len);

    let mut out_len = 0;

    // Check if path starts with drive letter (e.g., "C:")
    let has_drive = path_len >= 2 && src[1] == b':';
    if !has_drive && path_len > 0 && src[0] != b'\\' {
        // Prepend C:
        if out_len + 2 < buf.len() {
            buf[out_len] = b'C';
            out_len += 1;
            buf[out_len] = b':';
            out_len += 1;
        }
        // If path starts with /, it becomes C:\ which is correct
    }

    for i in 0..path_len {
        if out_len >= buf.len() - 1 {
            break;
        }
        buf[out_len] = if src[i] == b'/' { b'\\' } else { src[i] };
        out_len += 1;
    }
    buf[out_len] = 0;
    out_len
}

unsafe fn alloc_file() -> *mut FILE {
    for i in 3..MAX_FILES {
        if !FILE_TABLE[i].is_open {
            return &raw mut FILE_TABLE[i];
        }
    }
    ptr::null_mut()
}

// ---- Public API ----

#[no_mangle]
pub unsafe extern "C" fn fopen(path: *const c_char, mode: *const c_char) -> *mut FILE {
    if path.is_null() {
        return ptr::null_mut();
    }

    let f = alloc_file();
    if f.is_null() {
        return ptr::null_mut();
    }

    let handle = create_file_handle();

    // Translate path
    let mut path_buf = [0u8; 256];
    let path_len = translate_path(path, &mut path_buf);

    // Open the file
    let result = io_sync(
        handle,
        ASYNC_OP_OPEN,
        path_buf.as_ptr() as u32,
        path_len as u32,
        0,
    );

    if result.is_err() {
        // Close the handle we just created
        io_sync(handle, ASYNC_OP_CLOSE, 0, 0, 0).ok();
        return ptr::null_mut();
    }

    (*f).handle = handle;
    (*f).pos = 0;
    (*f).error = 0;
    (*f).eof = 0;
    (*f).is_open = true;
    (*f).is_console = false;
    (*f).unget = -1;

    // If mode contains 'a' (append), seek to end
    let mut m = mode;
    while !m.is_null() && *m != 0 {
        if *m as u8 == b'a' {
            // Get file size via stat and seek to end
            let size = file_size(handle);
            (*f).pos = size;
            break;
        }
        m = m.add(1);
    }

    f
}

unsafe fn file_size(handle: Handle) -> u32 {
    // Use stat ioctl to get file size
    let mut stat_buf = [0u32; 4]; // [size, ...]
    let result = io_sync(
        handle,
        FILE_OP_STAT,
        stat_buf.as_mut_ptr() as u32,
        core::mem::size_of_val(&stat_buf) as u32,
        0,
    );
    match result {
        Ok(_) => stat_buf[0],
        Err(_) => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn freopen(
    path: *const c_char,
    mode: *const c_char,
    stream: *mut FILE,
) -> *mut FILE {
    if stream.is_null() {
        return ptr::null_mut();
    }
    // Close the existing stream
    if (*stream).is_open && !(*stream).is_console {
        io_sync((*stream).handle, ASYNC_OP_CLOSE, 0, 0, 0).ok();
    }
    if path.is_null() {
        // NULL path means just change mode on the same fd — not meaningful for us
        return stream;
    }
    let handle = create_file_handle();
    let mut path_buf = [0u8; 256];
    let path_len = translate_path(path, &mut path_buf);
    let result = io_sync(
        handle,
        ASYNC_OP_OPEN,
        path_buf.as_ptr() as u32,
        path_len as u32,
        0,
    );
    if result.is_err() {
        io_sync(handle, ASYNC_OP_CLOSE, 0, 0, 0).ok();
        (*stream).is_open = false;
        return ptr::null_mut();
    }
    (*stream).handle = handle;
    (*stream).pos = 0;
    (*stream).error = 0;
    (*stream).eof = 0;
    (*stream).is_open = true;
    (*stream).is_console = false;
    (*stream).unget = -1;
    stream
}

#[no_mangle]
pub unsafe extern "C" fn fclose(f: *mut FILE) -> c_int {
    if f.is_null() || !(*f).is_open {
        return EOF;
    }
    if !(*f).is_console {
        io_sync((*f).handle, ASYNC_OP_CLOSE, 0, 0, 0).ok();
    }
    (*f).is_open = false;
    0
}

#[no_mangle]
pub unsafe extern "C" fn fread(
    ptr: *mut c_void,
    size: usize,
    nmemb: usize,
    f: *mut FILE,
) -> usize {
    if f.is_null() || !(*f).is_open || size == 0 || nmemb == 0 {
        return 0;
    }

    let total = size * nmemb;
    let result = io_sync(
        (*f).handle,
        ASYNC_OP_READ,
        ptr as u32,
        total as u32,
        (*f).pos,
    );

    match result {
        Ok(bytes_read) => {
            if bytes_read == 0 {
                (*f).eof = 1;
                0
            } else {
                (*f).pos += bytes_read;
                (bytes_read as usize) / size
            }
        }
        Err(_) => {
            (*f).error = 1;
            0
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn fwrite(
    ptr: *const c_void,
    size: usize,
    nmemb: usize,
    f: *mut FILE,
) -> usize {
    if f.is_null() || !(*f).is_open || size == 0 || nmemb == 0 {
        return 0;
    }

    let total = size * nmemb;
    let result = io_sync(
        (*f).handle,
        ASYNC_OP_WRITE,
        ptr as u32,
        total as u32,
        (*f).pos,
    );

    match result {
        Ok(bytes_written) => {
            (*f).pos += bytes_written;
            (bytes_written as usize) / size
        }
        Err(_) => {
            (*f).error = 1;
            0
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn fseek(f: *mut FILE, offset: c_int, whence: c_int) -> c_int {
    if f.is_null() || !(*f).is_open {
        return -1;
    }

    (*f).eof = 0;
    (*f).unget = -1;

    match whence {
        0 => {
            // SEEK_SET
            (*f).pos = offset as u32;
        }
        1 => {
            // SEEK_CUR
            (*f).pos = ((*f).pos as i32 + offset) as u32;
        }
        2 => {
            // SEEK_END
            let size = file_size((*f).handle);
            (*f).pos = (size as i32 + offset) as u32;
        }
        _ => return -1,
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn ftell(f: *mut FILE) -> c_int {
    if f.is_null() || !(*f).is_open {
        return -1;
    }
    (*f).pos as c_int
}

#[no_mangle]
pub unsafe extern "C" fn feof(f: *mut FILE) -> c_int {
    if f.is_null() {
        return 0;
    }
    (*f).eof
}

#[no_mangle]
pub unsafe extern "C" fn ferror(f: *mut FILE) -> c_int {
    if f.is_null() {
        return 0;
    }
    (*f).error
}

#[no_mangle]
pub unsafe extern "C" fn clearerr(f: *mut FILE) {
    if !f.is_null() {
        (*f).error = 0;
        (*f).eof = 0;
    }
}

#[no_mangle]
pub unsafe extern "C" fn fflush(_f: *mut FILE) -> c_int {
    // No buffering in our implementation, so this is a no-op
    0
}

#[no_mangle]
pub unsafe extern "C" fn fgetc(f: *mut FILE) -> c_int {
    if f.is_null() || !(*f).is_open {
        return EOF;
    }

    // Check unget buffer
    if (*f).unget >= 0 {
        let c = (*f).unget;
        (*f).unget = -1;
        return c;
    }

    let mut byte: u8 = 0;
    let result = io_sync(
        (*f).handle,
        ASYNC_OP_READ,
        &raw mut byte as u32,
        1,
        (*f).pos,
    );
    match result {
        Ok(n) if n > 0 => {
            (*f).pos += 1;
            byte as c_int
        }
        _ => {
            (*f).eof = 1;
            EOF
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn getc(f: *mut FILE) -> c_int {
    fgetc(f)
}

#[no_mangle]
pub unsafe extern "C" fn ungetc(c: c_int, f: *mut FILE) -> c_int {
    if f.is_null() || c == EOF {
        return EOF;
    }
    (*f).unget = c;
    (*f).eof = 0;
    c
}

#[no_mangle]
pub unsafe extern "C" fn fgets(s: *mut c_char, n: c_int, f: *mut FILE) -> *mut c_char {
    if n <= 0 || f.is_null() {
        return ptr::null_mut();
    }
    let mut i = 0;
    let max = (n - 1) as usize;
    while i < max {
        let c = fgetc(f);
        if c == EOF {
            if i == 0 {
                return ptr::null_mut();
            }
            break;
        }
        *s.add(i) = c as c_char;
        i += 1;
        if c == b'\n' as c_int {
            break;
        }
    }
    *s.add(i) = 0;
    s
}

#[no_mangle]
pub unsafe extern "C" fn fputc(c: c_int, f: *mut FILE) -> c_int {
    if f.is_null() || !(*f).is_open {
        return EOF;
    }
    let byte = c as u8;
    let result = io_sync(
        (*f).handle,
        ASYNC_OP_WRITE,
        &byte as *const u8 as u32,
        1,
        (*f).pos,
    );
    match result {
        Ok(_) => {
            (*f).pos += 1;
            c
        }
        Err(_) => {
            (*f).error = 1;
            EOF
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn putchar(c: c_int) -> c_int {
    fputc(c, stdout)
}

#[no_mangle]
pub unsafe extern "C" fn putc(c: c_int, f: *mut FILE) -> c_int {
    fputc(c, f)
}

#[no_mangle]
pub unsafe extern "C" fn fputs(s: *const c_char, f: *mut FILE) -> c_int {
    if s.is_null() || f.is_null() {
        return EOF;
    }
    let len = crate::string::strlen(s);
    let written = fwrite(s as *const c_void, 1, len, f);
    if written == 0 && len > 0 {
        EOF
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn puts(s: *const c_char) -> c_int {
    let r = fputs(s, stdout);
    if r == EOF {
        return EOF;
    }
    fputc(b'\n' as c_int, stdout)
}

// ---- printf family ----

/// Internal printf engine that writes to a callback.
unsafe fn format_to(
    mut write_fn: impl FnMut(u8),
    fmt: *const c_char,
    mut args: VaList,
) -> c_int {
    let mut count: c_int = 0;
    let mut i = 0;

    let emit = |write_fn: &mut dyn FnMut(u8), count: &mut c_int, c: u8| {
        write_fn(c);
        *count += 1;
    };

    loop {
        let c = *fmt.add(i) as u8;
        if c == 0 {
            break;
        }

        if c != b'%' {
            emit(&mut write_fn, &mut count, c);
            i += 1;
            continue;
        }

        i += 1;
        let c = *fmt.add(i) as u8;
        if c == 0 {
            break;
        }

        // Parse flags
        let mut left_justify = false;
        let mut zero_pad = false;
        let mut plus_sign = false;
        let mut space_sign = false;
        let mut hash_flag = false;
        loop {
            let fc = *fmt.add(i) as u8;
            match fc {
                b'-' => {
                    left_justify = true;
                    i += 1;
                }
                b'0' => {
                    zero_pad = true;
                    i += 1;
                }
                b'+' => {
                    plus_sign = true;
                    i += 1;
                }
                b' ' => {
                    space_sign = true;
                    i += 1;
                }
                b'#' => {
                    hash_flag = true;
                    i += 1;
                }
                _ => break,
            }
        }

        // Parse width
        let mut width: usize = 0;
        if *fmt.add(i) as u8 == b'*' {
            width = args.arg::<c_int>() as usize;
            i += 1;
        } else {
            while (*fmt.add(i) as u8) >= b'0' && (*fmt.add(i) as u8) <= b'9' {
                width = width * 10 + (*fmt.add(i) as u8 - b'0') as usize;
                i += 1;
            }
        }

        // Parse precision
        let mut precision: Option<usize> = None;
        if *fmt.add(i) as u8 == b'.' {
            i += 1;
            let mut prec = 0usize;
            if *fmt.add(i) as u8 == b'*' {
                prec = args.arg::<c_int>() as usize;
                i += 1;
            } else {
                while (*fmt.add(i) as u8) >= b'0' && (*fmt.add(i) as u8) <= b'9' {
                    prec = prec * 10 + (*fmt.add(i) as u8 - b'0') as usize;
                    i += 1;
                }
            }
            precision = Some(prec);
        }

        // Parse length modifier
        let mut long_flag = false;
        let c = *fmt.add(i) as u8;
        if c == b'l' {
            long_flag = true;
            i += 1;
            if *fmt.add(i) as u8 == b'l' {
                i += 1; // ll treated same as l on 32-bit
            }
        } else if c == b'h' {
            i += 1;
            if *fmt.add(i) as u8 == b'h' {
                i += 1;
            }
        } else if c == b'z' || c == b'j' || c == b't' {
            i += 1;
            long_flag = true; // size_t is 32-bit
        }

        // Parse conversion
        let conv = *fmt.add(i) as u8;
        i += 1;

        match conv {
            b'd' | b'i' => {
                let val = if long_flag {
                    args.arg::<i32>()
                } else {
                    args.arg::<c_int>() as i32
                };
                let mut buf = [0u8; 12];
                let s = format_int(val as i64, 10, false, &mut buf);
                let prefix = if val < 0 {
                    b'-'
                } else if plus_sign {
                    b'+'
                } else if space_sign {
                    b' '
                } else {
                    0
                };
                let slen = s.len() + if prefix != 0 { 1 } else { 0 };
                let pad = if width > slen { width - slen } else { 0 };
                let pad_char = if zero_pad && !left_justify { b'0' } else { b' ' };

                if !left_justify && pad_char == b' ' {
                    for _ in 0..pad {
                        emit(&mut write_fn, &mut count, b' ');
                    }
                }
                if prefix != 0 {
                    emit(&mut write_fn, &mut count, prefix);
                }
                if !left_justify && pad_char == b'0' {
                    for _ in 0..pad {
                        emit(&mut write_fn, &mut count, b'0');
                    }
                }
                for &b in s {
                    emit(&mut write_fn, &mut count, b);
                }
                if left_justify {
                    for _ in 0..pad {
                        emit(&mut write_fn, &mut count, b' ');
                    }
                }
            }
            b'u' => {
                let val = args.arg::<u32>();
                let mut buf = [0u8; 12];
                let s = format_uint(val as u64, 10, false, &mut buf);
                let pad = if width > s.len() { width - s.len() } else { 0 };
                if !left_justify {
                    let pc = if zero_pad { b'0' } else { b' ' };
                    for _ in 0..pad {
                        emit(&mut write_fn, &mut count, pc);
                    }
                }
                for &b in s {
                    emit(&mut write_fn, &mut count, b);
                }
                if left_justify {
                    for _ in 0..pad {
                        emit(&mut write_fn, &mut count, b' ');
                    }
                }
            }
            b'x' | b'X' => {
                let val = args.arg::<u32>();
                let upper = conv == b'X';
                let mut buf = [0u8; 12];
                let s = format_uint(val as u64, 16, upper, &mut buf);
                let prefix_len = if hash_flag && val != 0 { 2 } else { 0 };
                let slen = s.len() + prefix_len;
                let pad = if width > slen { width - slen } else { 0 };

                if !left_justify && !zero_pad {
                    for _ in 0..pad {
                        emit(&mut write_fn, &mut count, b' ');
                    }
                }
                if hash_flag && val != 0 {
                    emit(&mut write_fn, &mut count, b'0');
                    emit(&mut write_fn, &mut count, if upper { b'X' } else { b'x' });
                }
                if !left_justify && zero_pad {
                    for _ in 0..pad {
                        emit(&mut write_fn, &mut count, b'0');
                    }
                }
                for &b in s {
                    emit(&mut write_fn, &mut count, b);
                }
                if left_justify {
                    for _ in 0..pad {
                        emit(&mut write_fn, &mut count, b' ');
                    }
                }
            }
            b'o' => {
                let val = args.arg::<u32>();
                let mut buf = [0u8; 16];
                let s = format_uint(val as u64, 8, false, &mut buf);
                for &b in s {
                    emit(&mut write_fn, &mut count, b);
                }
            }
            b'p' => {
                let val = args.arg::<u32>();
                emit(&mut write_fn, &mut count, b'0');
                emit(&mut write_fn, &mut count, b'x');
                let mut buf = [0u8; 12];
                let s = format_uint(val as u64, 16, false, &mut buf);
                for &b in s {
                    emit(&mut write_fn, &mut count, b);
                }
            }
            b's' => {
                let s = args.arg::<*const c_char>();
                if s.is_null() {
                    let null_str = b"(null)";
                    for &b in null_str {
                        emit(&mut write_fn, &mut count, b);
                    }
                } else {
                    let max_len = precision.unwrap_or(usize::MAX);
                    let slen = crate::string::strnlen(s, max_len);
                    let pad = if width > slen { width - slen } else { 0 };
                    if !left_justify {
                        for _ in 0..pad {
                            emit(&mut write_fn, &mut count, b' ');
                        }
                    }
                    for j in 0..slen {
                        emit(&mut write_fn, &mut count, *s.add(j) as u8);
                    }
                    if left_justify {
                        for _ in 0..pad {
                            emit(&mut write_fn, &mut count, b' ');
                        }
                    }
                }
            }
            b'c' => {
                let c = args.arg::<c_int>() as u8;
                emit(&mut write_fn, &mut count, c);
            }
            b'f' | b'F' | b'g' | b'G' | b'e' | b'E' => {
                let val = args.arg::<f64>();
                let prec = precision.unwrap_or(6);
                let mut buf = [0u8; 64];
                let len = format_float(val, prec, &mut buf);
                let s = &buf[..len];
                let pad = if width > s.len() { width - s.len() } else { 0 };
                if !left_justify {
                    let pc = if zero_pad { b'0' } else { b' ' };
                    for _ in 0..pad {
                        emit(&mut write_fn, &mut count, pc);
                    }
                }
                for &b in s {
                    emit(&mut write_fn, &mut count, b);
                }
                if left_justify {
                    for _ in 0..pad {
                        emit(&mut write_fn, &mut count, b' ');
                    }
                }
            }
            b'%' => {
                emit(&mut write_fn, &mut count, b'%');
            }
            b'n' => {
                let p = args.arg::<*mut c_int>();
                if !p.is_null() {
                    *p = count;
                }
            }
            _ => {
                // Unknown format, just output it
                emit(&mut write_fn, &mut count, b'%');
                emit(&mut write_fn, &mut count, conv);
            }
        }
    }

    count
}

fn format_int<'a>(mut val: i64, base: u64, upper: bool, buf: &'a mut [u8]) -> &'a [u8] {
    let negative = val < 0;
    let mut uval = if negative {
        (-(val as i64)) as u64
    } else {
        val as u64
    };
    let s = format_uint(uval, base, upper, buf);
    s
}

fn format_uint<'a>(mut val: u64, base: u64, upper: bool, buf: &'a mut [u8]) -> &'a [u8] {
    if val == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }
    let digits = if upper {
        b"0123456789ABCDEF"
    } else {
        b"0123456789abcdef"
    };
    let mut pos = buf.len();
    while val > 0 && pos > 0 {
        pos -= 1;
        buf[pos] = digits[(val % base) as usize];
        val /= base;
    }
    &buf[pos..]
}

fn format_float(val: f64, precision: usize, buf: &mut [u8]) -> usize {
    let mut pos = 0;

    if val < 0.0 {
        buf[pos] = b'-';
        pos += 1;
    }

    let val = if val < 0.0 { -val } else { val };

    // Integer part
    let int_part = val as u64;
    let mut int_buf = [0u8; 20];
    let int_str = format_uint(int_part, 10, false, &mut int_buf);
    for &b in int_str {
        if pos < buf.len() {
            buf[pos] = b;
            pos += 1;
        }
    }

    if precision > 0 {
        if pos < buf.len() {
            buf[pos] = b'.';
            pos += 1;
        }

        let mut frac = val - int_part as f64;
        for _ in 0..precision {
            frac *= 10.0;
            let digit = frac as u8;
            if pos < buf.len() {
                buf[pos] = b'0' + digit;
                pos += 1;
            }
            frac -= digit as f64;
        }
    }

    pos
}

// ---- printf / fprintf / sprintf / snprintf ----

#[no_mangle]
pub unsafe extern "C" fn vfprintf(f: *mut FILE, fmt: *const c_char, args: VaList) -> c_int {
    let mut f_ptr = f;
    format_to(
        |b| {
            fputc(b as c_int, f_ptr);
        },
        fmt,
        args,
    )
}

#[no_mangle]
pub unsafe extern "C" fn printf(fmt: *const c_char, mut args: ...) -> c_int {
    vfprintf(stdout, fmt, args.as_va_list())
}

#[no_mangle]
pub unsafe extern "C" fn fprintf(f: *mut FILE, fmt: *const c_char, mut args: ...) -> c_int {
    vfprintf(f, fmt, args.as_va_list())
}

#[no_mangle]
pub unsafe extern "C" fn vsnprintf(
    buf: *mut c_char,
    size: usize,
    fmt: *const c_char,
    args: VaList,
) -> c_int {
    if size == 0 {
        // Still need to count
        let mut count = 0i32;
        format_to(|_| { count += 1; }, fmt, args);
        return count;
    }

    let mut pos = 0usize;
    let max = size - 1;
    let result = format_to(
        |b| {
            if pos < max {
                *buf.add(pos) = b as c_char;
                pos += 1;
            }
        },
        fmt,
        args,
    );
    *buf.add(pos) = 0;
    result
}

#[no_mangle]
pub unsafe extern "C" fn snprintf(
    buf: *mut c_char,
    size: usize,
    fmt: *const c_char,
    mut args: ...
) -> c_int {
    vsnprintf(buf, size, fmt, args.as_va_list())
}

#[no_mangle]
pub unsafe extern "C" fn sprintf(buf: *mut c_char, fmt: *const c_char, mut args: ...) -> c_int {
    vsnprintf(buf, usize::MAX, fmt, args.as_va_list())
}

#[no_mangle]
pub unsafe extern "C" fn vsprintf(
    buf: *mut c_char,
    fmt: *const c_char,
    args: VaList,
) -> c_int {
    vsnprintf(buf, usize::MAX, fmt, args)
}

#[no_mangle]
pub unsafe extern "C" fn vprintf(fmt: *const c_char, args: VaList) -> c_int {
    vfprintf(stdout, fmt, args)
}

// ---- sscanf (basic) ----

#[no_mangle]
pub unsafe extern "C" fn sscanf(s: *const c_char, fmt: *const c_char, mut args: ...) -> c_int {
    vsscanf(s, fmt, args.as_va_list())
}

#[no_mangle]
pub unsafe extern "C" fn vsscanf(s: *const c_char, fmt: *const c_char, mut args: VaList) -> c_int {
    let mut si = 0usize; // position in s
    let mut fi = 0usize; // position in fmt
    let mut matched: c_int = 0;

    loop {
        let fc = *fmt.add(fi) as u8;
        if fc == 0 {
            break;
        }

        if fc == b' ' || fc == b'\t' || fc == b'\n' {
            // Skip whitespace in both
            fi += 1;
            while crate::ctype::isspace(*s.add(si) as c_int) != 0 {
                si += 1;
            }
            continue;
        }

        if fc != b'%' {
            // Literal match
            if *s.add(si) as u8 != fc {
                break;
            }
            fi += 1;
            si += 1;
            continue;
        }

        fi += 1; // skip %
        let conv = *fmt.add(fi) as u8;
        fi += 1;

        match conv {
            b'd' | b'i' => {
                // Skip whitespace
                while crate::ctype::isspace(*s.add(si) as c_int) != 0 {
                    si += 1;
                }
                let start = si;
                if *s.add(si) as u8 == b'-' || *s.add(si) as u8 == b'+' {
                    si += 1;
                }
                while (*s.add(si) as u8) >= b'0' && (*s.add(si) as u8) <= b'9' {
                    si += 1;
                }
                if si == start {
                    break;
                }
                let p = args.arg::<*mut c_int>();
                *p = crate::stdlib::atoi(s.add(start));
                matched += 1;
            }
            b's' => {
                while crate::ctype::isspace(*s.add(si) as c_int) != 0 {
                    si += 1;
                }
                let dest = args.arg::<*mut c_char>();
                let mut di = 0;
                while *s.add(si) != 0 && crate::ctype::isspace(*s.add(si) as c_int) == 0 {
                    *dest.add(di) = *s.add(si);
                    di += 1;
                    si += 1;
                }
                *dest.add(di) = 0;
                matched += 1;
            }
            _ => break,
        }
    }

    matched
}

// ---- debug helpers (write directly to console handle) ----

/// Write raw bytes to stdout's kernel handle (Handle 1).
/// Used for early debug output before full stdio is needed.
pub unsafe fn debug_write(bytes: &[u8]) {
    let handle = Handle::new(1);
    io_sync(handle, ASYNC_OP_WRITE, bytes.as_ptr() as u32, bytes.len() as u32, 0).ok();
}

/// Write a u32 value as hex to stdout's kernel handle.
pub unsafe fn debug_write_hex(val: u32) {
    let digits = b"0123456789ABCDEF";
    let mut buf = [b'0'; 10]; // "0x" + 8 hex digits
    buf[0] = b'0';
    buf[1] = b'x';
    for i in 0..8 {
        let nibble = (val >> (28 - i * 4)) & 0xF;
        buf[2 + i] = digits[nibble as usize];
    }
    debug_write(&buf);
}

// ---- remove / rename ----

#[no_mangle]
pub unsafe extern "C" fn remove(_path: *const c_char) -> c_int {
    -1 // stub
}

#[no_mangle]
pub unsafe extern "C" fn rename(_old: *const c_char, _new: *const c_char) -> c_int {
    -1 // stub
}

// ---- tmpfile ----

#[no_mangle]
pub unsafe extern "C" fn tmpfile() -> *mut FILE {
    ptr::null_mut() // stub
}

static mut TMPNAM_COUNTER: u32 = 0;

#[no_mangle]
pub unsafe extern "C" fn tmpnam(s: *mut c_char) -> *mut c_char {
    static mut BUF: [u8; 20] = [0; 20];
    TMPNAM_COUNTER += 1;
    let dest = if s.is_null() { BUF.as_mut_ptr() as *mut c_char } else { s };
    // Generate "C:\TMP\tXXXXX"
    let prefix = b"C:\\TMP\\t";
    let mut i = 0;
    for &b in prefix {
        *dest.add(i) = b as c_char;
        i += 1;
    }
    // Append counter as decimal
    let mut num = TMPNAM_COUNTER;
    let start = i;
    loop {
        *dest.add(i) = b'0' as c_char + (num % 10) as c_char;
        i += 1;
        num /= 10;
        if num == 0 { break; }
    }
    // Reverse the digits
    let mut a = start;
    let mut b = i - 1;
    while a < b {
        let tmp = *dest.add(a);
        *dest.add(a) = *dest.add(b);
        *dest.add(b) = tmp;
        a += 1;
        b -= 1;
    }
    *dest.add(i) = 0;
    dest
}

// ---- fileno ----

#[no_mangle]
pub unsafe extern "C" fn fileno(f: *mut FILE) -> c_int {
    if f.is_null() {
        return -1;
    }
    (*f).handle.as_u32() as c_int
}

// ---- setbuf / setvbuf ----

#[no_mangle]
pub unsafe extern "C" fn setbuf(_f: *mut FILE, _buf: *mut c_char) {
    // no-op
}

#[no_mangle]
pub unsafe extern "C" fn setvbuf(_f: *mut FILE, _buf: *mut c_char, _mode: c_int, _size: usize) -> c_int {
    0 // success, no-op
}

// ---- rewind / fgetpos / fsetpos ----

#[no_mangle]
pub unsafe extern "C" fn rewind(f: *mut FILE) {
    fseek(f, 0, SEEK_SET);
    if !f.is_null() {
        (*f).error = 0;
    }
}

// ---- perror ----

#[no_mangle]
pub unsafe extern "C" fn perror(s: *const c_char) {
    if !s.is_null() && *s != 0 {
        fputs(s, stderr);
        fputs(b": \0".as_ptr() as *const c_char, stderr);
    }
    fputs(b"error\n\0".as_ptr() as *const c_char, stderr);
}
