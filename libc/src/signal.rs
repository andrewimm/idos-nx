//! Signal handling stubs.

use core::ffi::c_int;

pub type sighandler_t = unsafe extern "C" fn(c_int);

pub const SIG_DFL: sighandler_t = sig_dfl;
pub const SIG_IGN: sighandler_t = sig_ign;

unsafe extern "C" fn sig_dfl(_sig: c_int) {}
unsafe extern "C" fn sig_ign(_sig: c_int) {}

pub const SIGINT: c_int = 2;
pub const SIGTERM: c_int = 15;

#[no_mangle]
pub unsafe extern "C" fn signal(_sig: c_int, _handler: sighandler_t) -> sighandler_t {
    sig_dfl
}

#[no_mangle]
pub unsafe extern "C" fn raise(_sig: c_int) -> c_int {
    0
}
