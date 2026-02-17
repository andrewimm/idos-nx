use core::ffi::c_int;

#[no_mangle]
pub extern "C" fn isdigit(c: c_int) -> c_int {
    ((c as u8) >= b'0' && (c as u8) <= b'9') as c_int
}

#[no_mangle]
pub extern "C" fn isalpha(c: c_int) -> c_int {
    let c = c as u8;
    ((c >= b'A' && c <= b'Z') || (c >= b'a' && c <= b'z')) as c_int
}

#[no_mangle]
pub extern "C" fn isalnum(c: c_int) -> c_int {
    (isalpha(c) != 0 || isdigit(c) != 0) as c_int
}

#[no_mangle]
pub extern "C" fn isspace(c: c_int) -> c_int {
    let c = c as u8;
    (c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' || c == 0x0b || c == 0x0c) as c_int
}

#[no_mangle]
pub extern "C" fn isupper(c: c_int) -> c_int {
    let c = c as u8;
    (c >= b'A' && c <= b'Z') as c_int
}

#[no_mangle]
pub extern "C" fn islower(c: c_int) -> c_int {
    let c = c as u8;
    (c >= b'a' && c <= b'z') as c_int
}

#[no_mangle]
pub extern "C" fn toupper(c: c_int) -> c_int {
    if islower(c) != 0 {
        c - 32
    } else {
        c
    }
}

#[no_mangle]
pub extern "C" fn tolower(c: c_int) -> c_int {
    if isupper(c) != 0 {
        c + 32
    } else {
        c
    }
}

#[no_mangle]
pub extern "C" fn isprint(c: c_int) -> c_int {
    let c = c as u8;
    (c >= 0x20 && c <= 0x7e) as c_int
}

#[no_mangle]
pub extern "C" fn isxdigit(c: c_int) -> c_int {
    let c = c as u8;
    ((c >= b'0' && c <= b'9') || (c >= b'A' && c <= b'F') || (c >= b'a' && c <= b'f')) as c_int
}

#[no_mangle]
pub extern "C" fn iscntrl(c: c_int) -> c_int {
    let c = c as u8;
    (c < 0x20 || c == 0x7f) as c_int
}

#[no_mangle]
pub extern "C" fn isgraph(c: c_int) -> c_int {
    let c = c as u8;
    (c > 0x20 && c <= 0x7e) as c_int
}

#[no_mangle]
pub extern "C" fn ispunct(c: c_int) -> c_int {
    (isgraph(c) != 0 && isalnum(c) == 0) as c_int
}
