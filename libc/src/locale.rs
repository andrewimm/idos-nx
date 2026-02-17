//! Locale stubs.

use core::ffi::c_char;

static C_LOCALE: [u8; 2] = *b"C\0";

#[no_mangle]
pub unsafe extern "C" fn setlocale(_category: i32, _locale: *const c_char) -> *mut c_char {
    C_LOCALE.as_ptr() as *mut c_char
}
