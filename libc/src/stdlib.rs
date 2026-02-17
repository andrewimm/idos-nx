//! C stdlib functions: exit, atexit, atoi, strtol, qsort, rand, etc.

use core::ffi::{c_char, c_int, c_long, c_ulong, c_void};
use core::ptr;

// ---- atexit ----

const MAX_ATEXIT: usize = 32;
static mut ATEXIT_FUNCS: [Option<unsafe extern "C" fn()>; MAX_ATEXIT] = [None; MAX_ATEXIT];
static mut ATEXIT_COUNT: usize = 0;

#[no_mangle]
pub unsafe extern "C" fn atexit(func: unsafe extern "C" fn()) -> c_int {
    if ATEXIT_COUNT >= MAX_ATEXIT {
        return -1;
    }
    ATEXIT_FUNCS[ATEXIT_COUNT] = Some(func);
    ATEXIT_COUNT += 1;
    0
}

#[no_mangle]
pub unsafe extern "C" fn exit(status: c_int) -> ! {
    // Call atexit functions in reverse order
    while ATEXIT_COUNT > 0 {
        ATEXIT_COUNT -= 1;
        if let Some(func) = ATEXIT_FUNCS[ATEXIT_COUNT] {
            func();
        }
    }
    idos_api::syscall::exec::terminate(status as u32)
}

#[no_mangle]
pub unsafe extern "C" fn abort() -> ! {
    idos_api::syscall::exec::terminate(0xfe)
}

#[no_mangle]
pub unsafe extern "C" fn _exit(status: c_int) -> ! {
    idos_api::syscall::exec::terminate(status as u32)
}

// ---- numeric conversion ----

#[no_mangle]
pub unsafe extern "C" fn atoi(s: *const c_char) -> c_int {
    let mut i = 0;
    // skip whitespace
    while crate::ctype::isspace(*s.add(i) as c_int) != 0 {
        i += 1;
    }
    let mut neg = false;
    if *s.add(i) as u8 == b'-' {
        neg = true;
        i += 1;
    } else if *s.add(i) as u8 == b'+' {
        i += 1;
    }
    let mut result: c_int = 0;
    while (*s.add(i) as u8) >= b'0' && (*s.add(i) as u8) <= b'9' {
        result = result.wrapping_mul(10).wrapping_add((*s.add(i) as u8 - b'0') as c_int);
        i += 1;
    }
    if neg { -result } else { result }
}

#[no_mangle]
pub unsafe extern "C" fn atol(s: *const c_char) -> c_long {
    atoi(s) as c_long
}

#[no_mangle]
pub unsafe extern "C" fn atof(s: *const c_char) -> f64 {
    strtod(s, ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn strtol(
    s: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_long {
    let mut i = 0;
    while crate::ctype::isspace(*s.add(i) as c_int) != 0 {
        i += 1;
    }
    let mut neg = false;
    if *s.add(i) as u8 == b'-' {
        neg = true;
        i += 1;
    } else if *s.add(i) as u8 == b'+' {
        i += 1;
    }

    let mut base = base as u32;
    if base == 0 {
        if *s.add(i) as u8 == b'0' {
            if *s.add(i + 1) as u8 == b'x' || *s.add(i + 1) as u8 == b'X' {
                base = 16;
                i += 2;
            } else {
                base = 8;
                i += 1;
            }
        } else {
            base = 10;
        }
    } else if base == 16 {
        if *s.add(i) as u8 == b'0'
            && (*s.add(i + 1) as u8 == b'x' || *s.add(i + 1) as u8 == b'X')
        {
            i += 2;
        }
    }

    let mut result: c_long = 0;
    let start = i;
    loop {
        let c = *s.add(i) as u8;
        let digit = if c >= b'0' && c <= b'9' {
            (c - b'0') as u32
        } else if c >= b'a' && c <= b'z' {
            (c - b'a' + 10) as u32
        } else if c >= b'A' && c <= b'Z' {
            (c - b'A' + 10) as u32
        } else {
            break;
        };
        if digit >= base {
            break;
        }
        result = result.wrapping_mul(base as c_long).wrapping_add(digit as c_long);
        i += 1;
    }

    if !endptr.is_null() {
        *endptr = s.add(i) as *mut c_char;
    }
    if neg { -result } else { result }
}

#[no_mangle]
pub unsafe extern "C" fn strtoul(
    s: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_ulong {
    // Reuse strtol and cast
    strtol(s, endptr, base) as c_ulong
}

#[no_mangle]
pub unsafe extern "C" fn strtod(s: *const c_char, endptr: *mut *mut c_char) -> f64 {
    let mut i = 0;
    while crate::ctype::isspace(*s.add(i) as c_int) != 0 {
        i += 1;
    }
    let mut neg = false;
    if *s.add(i) as u8 == b'-' {
        neg = true;
        i += 1;
    } else if *s.add(i) as u8 == b'+' {
        i += 1;
    }

    let mut result: f64 = 0.0;
    // Integer part
    while (*s.add(i) as u8) >= b'0' && (*s.add(i) as u8) <= b'9' {
        result = result * 10.0 + (*s.add(i) as u8 - b'0') as f64;
        i += 1;
    }
    // Fractional part
    if *s.add(i) as u8 == b'.' {
        i += 1;
        let mut frac = 0.1;
        while (*s.add(i) as u8) >= b'0' && (*s.add(i) as u8) <= b'9' {
            result += (*s.add(i) as u8 - b'0') as f64 * frac;
            frac *= 0.1;
            i += 1;
        }
    }
    // Exponent
    if *s.add(i) as u8 == b'e' || *s.add(i) as u8 == b'E' {
        i += 1;
        let mut exp_neg = false;
        if *s.add(i) as u8 == b'-' {
            exp_neg = true;
            i += 1;
        } else if *s.add(i) as u8 == b'+' {
            i += 1;
        }
        let mut exp: i32 = 0;
        while (*s.add(i) as u8) >= b'0' && (*s.add(i) as u8) <= b'9' {
            exp = exp * 10 + (*s.add(i) as u8 - b'0') as i32;
            i += 1;
        }
        if exp_neg {
            exp = -exp;
        }
        // pow10
        let mut factor = 1.0f64;
        let mut e = if exp < 0 { -exp } else { exp } as u32;
        let mut base = 10.0f64;
        while e > 0 {
            if e & 1 != 0 {
                factor *= base;
            }
            base *= base;
            e >>= 1;
        }
        if exp < 0 {
            result /= factor;
        } else {
            result *= factor;
        }
    }

    if !endptr.is_null() {
        *endptr = s.add(i) as *mut c_char;
    }
    if neg { -result } else { result }
}

#[no_mangle]
pub unsafe extern "C" fn strtof(s: *const c_char, endptr: *mut *mut c_char) -> f32 {
    strtod(s, endptr) as f32
}

// ---- abs / rand ----

#[no_mangle]
pub extern "C" fn abs(x: c_int) -> c_int {
    if x < 0 { -x } else { x }
}

#[no_mangle]
pub extern "C" fn labs(x: c_long) -> c_long {
    if x < 0 { -x } else { x }
}

static mut RAND_SEED: u32 = 1;

#[no_mangle]
pub unsafe extern "C" fn srand(seed: u32) {
    RAND_SEED = seed;
}

#[no_mangle]
pub unsafe extern "C" fn rand() -> c_int {
    // LCG from glibc
    RAND_SEED = RAND_SEED.wrapping_mul(1103515245).wrapping_add(12345);
    ((RAND_SEED >> 16) & 0x7fff) as c_int
}

// ---- qsort / bsearch ----

#[no_mangle]
pub unsafe extern "C" fn qsort(
    base: *mut c_void,
    nmemb: usize,
    size: usize,
    compar: unsafe extern "C" fn(*const c_void, *const c_void) -> c_int,
) {
    // Simple insertion sort (good enough for small arrays, correct for all)
    if nmemb <= 1 {
        return;
    }
    let base = base as *mut u8;
    // Allocate temp buffer on stack for elements up to 256 bytes, heap otherwise
    let mut tmp_buf = [0u8; 256];
    let tmp: *mut u8 = if size <= 256 {
        tmp_buf.as_mut_ptr()
    } else {
        crate::allocator::malloc(size)
    };

    for i in 1..nmemb {
        let elem = base.add(i * size);
        // Copy current element to tmp
        ptr::copy_nonoverlapping(elem, tmp, size);
        let mut j = i;
        while j > 0 {
            let prev = base.add((j - 1) * size);
            if compar(prev as *const c_void, tmp as *const c_void) <= 0 {
                break;
            }
            ptr::copy_nonoverlapping(prev, base.add(j * size), size);
            j -= 1;
        }
        ptr::copy_nonoverlapping(tmp, base.add(j * size), size);
    }

    if size > 256 {
        crate::allocator::free(tmp);
    }
}

#[no_mangle]
pub unsafe extern "C" fn bsearch(
    key: *const c_void,
    base: *const c_void,
    nmemb: usize,
    size: usize,
    compar: unsafe extern "C" fn(*const c_void, *const c_void) -> c_int,
) -> *mut c_void {
    let base = base as *const u8;
    let mut lo = 0usize;
    let mut hi = nmemb;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let elem = base.add(mid * size) as *const c_void;
        let cmp = compar(key, elem);
        if cmp == 0 {
            return elem as *mut c_void;
        } else if cmp < 0 {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    ptr::null_mut()
}

// ---- getenv ----

#[no_mangle]
pub unsafe extern "C" fn getenv(_name: *const c_char) -> *mut c_char {
    ptr::null_mut()
}

// ---- system ----

#[no_mangle]
pub unsafe extern "C" fn system(_command: *const c_char) -> c_int {
    -1
}
