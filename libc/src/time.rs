//! Minimal time functions.

use core::ffi::c_int;

pub type time_t = i32;
pub type clock_t = u32;

#[repr(C)]
pub struct tm {
    pub tm_sec: c_int,
    pub tm_min: c_int,
    pub tm_hour: c_int,
    pub tm_mday: c_int,
    pub tm_mon: c_int,
    pub tm_year: c_int,
    pub tm_wday: c_int,
    pub tm_yday: c_int,
    pub tm_isdst: c_int,
}

static mut TM_BUF: tm = tm {
    tm_sec: 0,
    tm_min: 0,
    tm_hour: 0,
    tm_mday: 1,
    tm_mon: 0,
    tm_year: 95, // 1995
    tm_wday: 0,
    tm_yday: 0,
    tm_isdst: 0,
};

#[no_mangle]
pub unsafe extern "C" fn time(tloc: *mut time_t) -> time_t {
    let ms = idos_api::syscall::time::get_monotonic_ms();
    let t = (ms / 1000) as time_t;
    if !tloc.is_null() {
        *tloc = t;
    }
    t
}

#[no_mangle]
pub unsafe extern "C" fn clock() -> clock_t {
    // CLOCKS_PER_SEC is typically 1000000 on POSIX
    let ms = idos_api::syscall::time::get_monotonic_ms();
    (ms * 1000) as clock_t
}

#[no_mangle]
pub unsafe extern "C" fn difftime(time1: time_t, time0: time_t) -> f64 {
    (time1 - time0) as f64
}

#[no_mangle]
pub unsafe extern "C" fn localtime(timep: *const time_t) -> *mut tm {
    // Very minimal: just return a static tm with seconds filled in
    let t = if !timep.is_null() { *timep } else { 0 };
    TM_BUF.tm_sec = t % 60;
    TM_BUF.tm_min = (t / 60) % 60;
    TM_BUF.tm_hour = (t / 3600) % 24;
    &raw mut TM_BUF
}

#[no_mangle]
pub unsafe extern "C" fn gmtime(timep: *const time_t) -> *mut tm {
    localtime(timep)
}

#[no_mangle]
pub unsafe extern "C" fn mktime(tm: *mut tm) -> time_t {
    if tm.is_null() {
        return -1;
    }
    // Very minimal
    ((*tm).tm_hour * 3600 + (*tm).tm_min * 60 + (*tm).tm_sec) as time_t
}

#[no_mangle]
pub unsafe extern "C" fn strftime(
    s: *mut u8,
    maxsize: usize,
    _format: *const u8,
    _tm: *const tm,
) -> usize {
    // Stub: just return empty string
    if maxsize > 0 {
        *s = 0;
    }
    0
}
