//! C string and memory functions.
//!
//! Note: memcpy, memset, memcmp, memmove are provided by compiler-builtins-mem
//! so we don't redefine them here to avoid duplicate symbols.

use core::ffi::{c_char, c_int, c_void};
use core::ptr;

#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const c_char) -> usize {
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    len
}

#[no_mangle]
pub unsafe extern "C" fn strcmp(s1: *const c_char, s2: *const c_char) -> c_int {
    let mut i = 0;
    loop {
        let a = *s1.add(i) as u8;
        let b = *s2.add(i) as u8;
        if a != b {
            return a as c_int - b as c_int;
        }
        if a == 0 {
            return 0;
        }
        i += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn strncmp(s1: *const c_char, s2: *const c_char, n: usize) -> c_int {
    for i in 0..n {
        let a = *s1.add(i) as u8;
        let b = *s2.add(i) as u8;
        if a != b {
            return a as c_int - b as c_int;
        }
        if a == 0 {
            return 0;
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn strcpy(dest: *mut c_char, src: *const c_char) -> *mut c_char {
    let mut i = 0;
    loop {
        let c = *src.add(i);
        *dest.add(i) = c;
        if c == 0 {
            break;
        }
        i += 1;
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn strncpy(
    dest: *mut c_char,
    src: *const c_char,
    n: usize,
) -> *mut c_char {
    let mut i = 0;
    while i < n {
        let c = *src.add(i);
        *dest.add(i) = c;
        if c == 0 {
            // Pad remaining with zeros
            i += 1;
            while i < n {
                *dest.add(i) = 0;
                i += 1;
            }
            return dest;
        }
        i += 1;
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn strcat(dest: *mut c_char, src: *const c_char) -> *mut c_char {
    let dest_len = strlen(dest);
    strcpy(dest.add(dest_len), src);
    dest
}

#[no_mangle]
pub unsafe extern "C" fn strncat(
    dest: *mut c_char,
    src: *const c_char,
    n: usize,
) -> *mut c_char {
    let dest_len = strlen(dest);
    let mut i = 0;
    while i < n {
        let c = *src.add(i);
        if c == 0 {
            break;
        }
        *dest.add(dest_len + i) = c;
        i += 1;
    }
    *dest.add(dest_len + i) = 0;
    dest
}

#[no_mangle]
pub unsafe extern "C" fn strchr(s: *const c_char, c: c_int) -> *mut c_char {
    let c = c as u8;
    let mut p = s;
    loop {
        if *p as u8 == c {
            return p as *mut c_char;
        }
        if *p == 0 {
            return ptr::null_mut();
        }
        p = p.add(1);
    }
}

#[no_mangle]
pub unsafe extern "C" fn strrchr(s: *const c_char, c: c_int) -> *mut c_char {
    let c = c as u8;
    let mut last: *mut c_char = ptr::null_mut();
    let mut p = s;
    loop {
        if *p as u8 == c {
            last = p as *mut c_char;
        }
        if *p == 0 {
            return last;
        }
        p = p.add(1);
    }
}

#[no_mangle]
pub unsafe extern "C" fn strstr(haystack: *const c_char, needle: *const c_char) -> *mut c_char {
    if *needle == 0 {
        return haystack as *mut c_char;
    }
    let needle_len = strlen(needle);
    let mut p = haystack;
    while *p != 0 {
        if strncmp(p, needle, needle_len) == 0 {
            return p as *mut c_char;
        }
        p = p.add(1);
    }
    ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn strdup(s: *const c_char) -> *mut c_char {
    let len = strlen(s);
    let p = crate::allocator::malloc(len + 1) as *mut c_char;
    if !p.is_null() {
        ptr::copy_nonoverlapping(s as *const u8, p as *mut u8, len + 1);
    }
    p
}

#[no_mangle]
pub unsafe extern "C" fn strerror(errnum: c_int) -> *mut c_char {
    // Minimal: just return a generic string
    static mut BUF: [u8; 32] = *b"Error\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    BUF.as_mut_ptr() as *mut c_char
}

#[no_mangle]
pub unsafe extern "C" fn strcasecmp(s1: *const c_char, s2: *const c_char) -> c_int {
    let mut i = 0;
    loop {
        let a = to_lower(*s1.add(i) as u8);
        let b = to_lower(*s2.add(i) as u8);
        if a != b {
            return a as c_int - b as c_int;
        }
        if a == 0 {
            return 0;
        }
        i += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn strncasecmp(
    s1: *const c_char,
    s2: *const c_char,
    n: usize,
) -> c_int {
    for i in 0..n {
        let a = to_lower(*s1.add(i) as u8);
        let b = to_lower(*s2.add(i) as u8);
        if a != b {
            return a as c_int - b as c_int;
        }
        if a == 0 {
            return 0;
        }
    }
    0
}

fn to_lower(c: u8) -> u8 {
    if c >= b'A' && c <= b'Z' {
        c + 32
    } else {
        c
    }
}

static mut STRTOK_POS: *mut c_char = ptr::null_mut();

#[no_mangle]
pub unsafe extern "C" fn strtok(s: *mut c_char, delim: *const c_char) -> *mut c_char {
    let mut p = if !s.is_null() { s } else { STRTOK_POS };
    if p.is_null() {
        return ptr::null_mut();
    }

    // Skip leading delimiters
    while *p != 0 && is_delim(*p as u8, delim) {
        p = p.add(1);
    }
    if *p == 0 {
        STRTOK_POS = ptr::null_mut();
        return ptr::null_mut();
    }

    let token = p;
    // Find end of token
    while *p != 0 && !is_delim(*p as u8, delim) {
        p = p.add(1);
    }
    if *p != 0 {
        *p = 0;
        STRTOK_POS = p.add(1);
    } else {
        STRTOK_POS = ptr::null_mut();
    }
    token
}

unsafe fn is_delim(c: u8, delim: *const c_char) -> bool {
    let mut d = delim;
    while *d != 0 {
        if *d as u8 == c {
            return true;
        }
        d = d.add(1);
    }
    false
}

// snprintf and strnlen used by various code
#[no_mangle]
pub unsafe extern "C" fn strnlen(s: *const c_char, maxlen: usize) -> usize {
    let mut len = 0;
    while len < maxlen && *s.add(len) != 0 {
        len += 1;
    }
    len
}

#[no_mangle]
pub unsafe extern "C" fn stpcpy(dest: *mut c_char, src: *const c_char) -> *mut c_char {
    let mut i = 0;
    loop {
        let c = *src.add(i);
        *dest.add(i) = c;
        if c == 0 {
            return dest.add(i);
        }
        i += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn memchr(s: *const c_void, c: c_int, n: usize) -> *mut c_void {
    let s = s as *const u8;
    let c = c as u8;
    for i in 0..n {
        if *s.add(i) == c {
            return s.add(i) as *mut c_void;
        }
    }
    ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn memrchr(s: *const c_void, c: c_int, n: usize) -> *mut c_void {
    let s = s as *const u8;
    let c = c as u8;
    let mut i = n;
    while i > 0 {
        i -= 1;
        if *s.add(i) == c {
            return s.add(i) as *mut c_void;
        }
    }
    ptr::null_mut()
}
