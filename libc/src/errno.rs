use core::ffi::c_int;

#[no_mangle]
pub static mut errno: c_int = 0;

// Some C code accesses errno via a function returning a pointer (e.g. __errno_location)
#[no_mangle]
pub unsafe extern "C" fn __errno_location() -> *mut c_int {
    &raw mut errno
}

// errno constants
pub const ENOENT: c_int = 2;
pub const EIO: c_int = 5;
pub const EBADF: c_int = 9;
pub const ENOMEM: c_int = 12;
pub const EACCES: c_int = 13;
pub const EEXIST: c_int = 17;
pub const ENOTDIR: c_int = 20;
pub const EINVAL: c_int = 22;
pub const EMFILE: c_int = 24;
pub const ENOSPC: c_int = 28;
pub const ERANGE: c_int = 34;
pub const ENOSYS: c_int = 38;
