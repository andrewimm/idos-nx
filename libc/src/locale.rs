//! Locale stubs.

use core::ffi::c_char;

static C_LOCALE: [u8; 2] = *b"C\0";
static DOT: [u8; 2] = *b".\0";
static EMPTY: [u8; 1] = *b"\0";

#[repr(C)]
struct LConv {
    decimal_point: *const c_char,
    thousands_sep: *const c_char,
    grouping: *const c_char,
    int_curr_symbol: *const c_char,
    currency_symbol: *const c_char,
    mon_decimal_point: *const c_char,
    mon_thousands_sep: *const c_char,
    mon_grouping: *const c_char,
    positive_sign: *const c_char,
    negative_sign: *const c_char,
}

unsafe impl Sync for LConv {}

static LCONV: LConv = LConv {
    decimal_point: DOT.as_ptr() as *const c_char,
    thousands_sep: EMPTY.as_ptr() as *const c_char,
    grouping: EMPTY.as_ptr() as *const c_char,
    int_curr_symbol: EMPTY.as_ptr() as *const c_char,
    currency_symbol: EMPTY.as_ptr() as *const c_char,
    mon_decimal_point: EMPTY.as_ptr() as *const c_char,
    mon_thousands_sep: EMPTY.as_ptr() as *const c_char,
    mon_grouping: EMPTY.as_ptr() as *const c_char,
    positive_sign: EMPTY.as_ptr() as *const c_char,
    negative_sign: EMPTY.as_ptr() as *const c_char,
};

#[no_mangle]
pub unsafe extern "C" fn setlocale(_category: i32, _locale: *const c_char) -> *mut c_char {
    C_LOCALE.as_ptr() as *mut c_char
}

#[no_mangle]
pub unsafe extern "C" fn localeconv() -> *mut LConv {
    &LCONV as *const LConv as *mut LConv
}
