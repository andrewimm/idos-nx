//! mmap/munmap wrappers.

use core::ffi::{c_int, c_void};

use idos_api::syscall::memory::map_memory;

pub const PROT_READ: c_int = 1;
pub const PROT_WRITE: c_int = 2;
pub const PROT_EXEC: c_int = 4;
pub const MAP_PRIVATE: c_int = 0x02;
pub const MAP_ANONYMOUS: c_int = 0x20;
pub const MAP_FAILED: *mut c_void = !0usize as *mut c_void;

#[no_mangle]
pub unsafe extern "C" fn mmap(
    addr: *mut c_void,
    length: usize,
    _prot: c_int,
    _flags: c_int,
    _fd: c_int,
    _offset: i32,
) -> *mut c_void {
    let vaddr = if addr.is_null() {
        None
    } else {
        Some(addr as u32)
    };

    match map_memory(vaddr, length as u32, None) {
        Ok(mapped) => mapped as *mut c_void,
        Err(()) => MAP_FAILED,
    }
}

#[no_mangle]
pub unsafe extern "C" fn munmap(_addr: *mut c_void, _length: usize) -> c_int {
    // Kernel doesn't support unmapping yet
    0 // pretend success
}
