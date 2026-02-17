//! termios implementation using kernel TCGETS/TCSETS ioctls.

use core::ffi::c_int;
use core::mem;

use idos_api::io::sync::ioctl_sync;
use idos_api::io::termios::{Termios, TCGETS, TCSETS, TCSETSF, TCSETSW};

use crate::stdio;

const TCSANOW: c_int = 0;
const TCSADRAIN: c_int = 1;
const TCSAFLUSH: c_int = 2;

#[no_mangle]
pub unsafe extern "C" fn tcgetattr(fd: c_int, termios_p: *mut Termios) -> c_int {
    if termios_p.is_null() {
        return -1;
    }
    let Some(handle) = stdio::fd_handle(fd) else {
        return -1;
    };
    let ptr = termios_p as u32;
    let len = mem::size_of::<Termios>() as u32;
    match ioctl_sync(handle, TCGETS, ptr, len) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn tcsetattr(
    fd: c_int,
    optional_actions: c_int,
    termios_p: *const Termios,
) -> c_int {
    if termios_p.is_null() {
        return -1;
    }
    let Some(handle) = stdio::fd_handle(fd) else {
        return -1;
    };
    let ioctl_cmd = match optional_actions {
        TCSANOW => TCSETS,
        TCSADRAIN => TCSETSW,
        TCSAFLUSH => TCSETSF,
        _ => return -1,
    };
    let ptr = termios_p as u32;
    let len = mem::size_of::<Termios>() as u32;
    match ioctl_sync(handle, ioctl_cmd, ptr, len) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cfgetispeed(_termios_p: *const Termios) -> u32 {
    9600
}

#[no_mangle]
pub unsafe extern "C" fn cfgetospeed(_termios_p: *const Termios) -> u32 {
    9600
}

#[no_mangle]
pub unsafe extern "C" fn cfsetispeed(_termios_p: *mut Termios, _speed: u32) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn cfsetospeed(_termios_p: *mut Termios, _speed: u32) -> c_int {
    0
}
