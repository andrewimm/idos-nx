//! POSIX-like system calls.

use core::ffi::{c_char, c_int, c_void};

use idos_api::syscall::syscall;

#[no_mangle]
pub unsafe extern "C" fn sleep(seconds: u32) -> u32 {
    idos_api::syscall::time::sleep_ms(seconds * 1000);
    0
}

#[no_mangle]
pub unsafe extern "C" fn usleep(usec: u32) -> c_int {
    let ms = if usec < 1000 { 1 } else { usec / 1000 };
    idos_api::syscall::time::sleep_ms(ms);
    0
}

#[no_mangle]
pub unsafe extern "C" fn isatty(fd: c_int) -> c_int {
    // stdin/stdout/stderr are consoles
    if fd >= 0 && fd <= 2 { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn getcwd(buf: *mut c_char, size: usize) -> *mut c_char {
    // Stub: return "C:\"
    if buf.is_null() || size < 4 {
        return core::ptr::null_mut();
    }
    *buf.add(0) = b'C' as c_char;
    *buf.add(1) = b':' as c_char;
    *buf.add(2) = b'\\' as c_char;
    *buf.add(3) = 0;
    buf
}

#[no_mangle]
pub unsafe extern "C" fn chdir(_path: *const c_char) -> c_int {
    0 // stub success
}

#[no_mangle]
pub unsafe extern "C" fn access(_path: *const c_char, _mode: c_int) -> c_int {
    // Stub: always say file exists
    0
}

#[no_mangle]
pub unsafe extern "C" fn unlink(_path: *const c_char) -> c_int {
    -1 // stub failure
}

#[no_mangle]
pub unsafe extern "C" fn getpid() -> c_int {
    // Return current task ID
    syscall(0x03, 0, 0, 0) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn getuid() -> u32 {
    0
}

#[no_mangle]
pub unsafe extern "C" fn geteuid() -> u32 {
    0
}

// read/write/close as POSIX-like FD operations
// These work with raw kernel handles, not FILE*

#[no_mangle]
pub unsafe extern "C" fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize {
    use idos_api::io::{AsyncOp, Handle, ASYNC_OP_READ};
    use idos_api::syscall::exec::futex_wait_u32;
    use idos_api::syscall::io::append_io_op;
    use core::sync::atomic::Ordering;

    let handle = Handle::new(fd as u32);
    let op = AsyncOp::new(ASYNC_OP_READ, buf as u32, count as u32, 0);
    append_io_op(handle, &op, None);

    while op.signal.load(Ordering::SeqCst) == 0 {
        futex_wait_u32(&op.signal, 0, None);
    }

    let ret = op.return_value.load(Ordering::SeqCst);
    if ret & 0x80000000 != 0 {
        -1
    } else {
        ret as isize
    }
}

#[no_mangle]
pub unsafe extern "C" fn write(fd: c_int, buf: *const c_void, count: usize) -> isize {
    use idos_api::io::{AsyncOp, Handle, ASYNC_OP_WRITE};
    use idos_api::syscall::exec::futex_wait_u32;
    use idos_api::syscall::io::append_io_op;
    use core::sync::atomic::Ordering;

    let handle = Handle::new(fd as u32);
    let op = AsyncOp::new(ASYNC_OP_WRITE, buf as u32, count as u32, 0);
    append_io_op(handle, &op, None);

    while op.signal.load(Ordering::SeqCst) == 0 {
        futex_wait_u32(&op.signal, 0, None);
    }

    let ret = op.return_value.load(Ordering::SeqCst);
    if ret & 0x80000000 != 0 {
        -1
    } else {
        ret as isize
    }
}

#[no_mangle]
pub unsafe extern "C" fn close(fd: c_int) -> c_int {
    use idos_api::io::{AsyncOp, Handle, ASYNC_OP_CLOSE};
    use idos_api::syscall::exec::futex_wait_u32;
    use idos_api::syscall::io::append_io_op;
    use core::sync::atomic::Ordering;

    let handle = Handle::new(fd as u32);
    let op = AsyncOp::new(ASYNC_OP_CLOSE, 0, 0, 0);
    append_io_op(handle, &op, None);

    while op.signal.load(Ordering::SeqCst) == 0 {
        futex_wait_u32(&op.signal, 0, None);
    }

    0
}

#[no_mangle]
pub unsafe extern "C" fn lseek(_fd: c_int, _offset: i32, _whence: c_int) -> i32 {
    // Kernel handles are position-less (offset is passed per-operation)
    // This is a no-op; stdio tracks position itself
    0
}
