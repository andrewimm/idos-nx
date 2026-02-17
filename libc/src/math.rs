//! Math library using x87 FPU instructions.

use core::arch::asm;

#[no_mangle]
pub extern "C" fn sin(x: f64) -> f64 {
    let result: f64;
    unsafe {
        asm!(
            "fld qword ptr [{x}]",
            "fsin",
            "fstp qword ptr [{x}]",
            x = in(reg) &x as *const f64,
        );
    }
    x
}

#[no_mangle]
pub extern "C" fn cos(x: f64) -> f64 {
    let result: f64;
    unsafe {
        asm!(
            "fld qword ptr [{x}]",
            "fcos",
            "fstp qword ptr [{x}]",
            x = in(reg) &x as *const f64,
        );
    }
    x
}

#[no_mangle]
pub extern "C" fn tan(x: f64) -> f64 {
    let result: f64;
    unsafe {
        asm!(
            "fld qword ptr [{x}]",
            "fptan",
            "fstp st(0)",  // pop the 1.0 that fptan pushes
            "fstp qword ptr [{x}]",
            x = in(reg) &x as *const f64,
        );
    }
    x
}

#[no_mangle]
pub extern "C" fn atan(x: f64) -> f64 {
    let mut result: f64 = 0.0;
    unsafe {
        asm!(
            "fld qword ptr [{x}]",
            "fld1",
            "fpatan",
            "fstp qword ptr [{out}]",
            x = in(reg) &x as *const f64,
            out = in(reg) &mut result as *mut f64,
        );
    }
    result
}

#[no_mangle]
pub extern "C" fn atan2(y: f64, x: f64) -> f64 {
    let mut result: f64 = 0.0;
    unsafe {
        asm!(
            "fld qword ptr [{y}]",
            "fld qword ptr [{x}]",
            "fpatan",
            "fstp qword ptr [{out}]",
            y = in(reg) &y as *const f64,
            x = in(reg) &x as *const f64,
            out = in(reg) &mut result as *mut f64,
        );
    }
    result
}

#[no_mangle]
pub extern "C" fn sqrt(x: f64) -> f64 {
    let mut result: f64 = 0.0;
    unsafe {
        asm!(
            "fld qword ptr [{x}]",
            "fsqrt",
            "fstp qword ptr [{out}]",
            x = in(reg) &x as *const f64,
            out = in(reg) &mut result as *mut f64,
        );
    }
    result
}

#[no_mangle]
pub extern "C" fn fabs(x: f64) -> f64 {
    let mut result: f64 = 0.0;
    unsafe {
        asm!(
            "fld qword ptr [{x}]",
            "fabs",
            "fstp qword ptr [{out}]",
            x = in(reg) &x as *const f64,
            out = in(reg) &mut result as *mut f64,
        );
    }
    result
}

#[no_mangle]
pub extern "C" fn fabsf(x: f32) -> f32 {
    if x < 0.0 { -x } else { x }
}

#[no_mangle]
pub extern "C" fn floor(x: f64) -> f64 {
    let mut result: f64 = 0.0;
    let mut cw: u16 = 0;
    let mut new_cw: u16;
    unsafe {
        asm!(
            // Save current control word
            "fnstcw [{cw}]",
            cw = in(reg) &mut cw as *mut u16,
        );
        // Set rounding mode to round toward -infinity (bits 10-11 = 01)
        new_cw = (cw & 0xF3FF) | 0x0400;
        asm!(
            "fldcw [{cw}]",
            "fld qword ptr [{x}]",
            "frndint",
            "fstp qword ptr [{out}]",
            "fldcw [{old_cw}]",
            cw = in(reg) &new_cw as *const u16,
            x = in(reg) &x as *const f64,
            out = in(reg) &mut result as *mut f64,
            old_cw = in(reg) &cw as *const u16,
        );
    }
    result
}

#[no_mangle]
pub extern "C" fn ceil(x: f64) -> f64 {
    let mut result: f64 = 0.0;
    let mut cw: u16 = 0;
    let mut new_cw: u16;
    unsafe {
        asm!(
            "fnstcw [{cw}]",
            cw = in(reg) &mut cw as *mut u16,
        );
        // Set rounding mode to round toward +infinity (bits 10-11 = 10)
        new_cw = (cw & 0xF3FF) | 0x0800;
        asm!(
            "fldcw [{cw}]",
            "fld qword ptr [{x}]",
            "frndint",
            "fstp qword ptr [{out}]",
            "fldcw [{old_cw}]",
            cw = in(reg) &new_cw as *const u16,
            x = in(reg) &x as *const f64,
            out = in(reg) &mut result as *mut f64,
            old_cw = in(reg) &cw as *const u16,
        );
    }
    result
}

#[no_mangle]
pub extern "C" fn fmod(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return 0.0; // or NaN, but keep it simple
    }
    let mut result: f64 = 0.0;
    unsafe {
        asm!(
            "fld qword ptr [{y}]",
            "fld qword ptr [{x}]",
            "2:",
            "fprem",
            "fnstsw ax",
            "test ah, 4",  // check C2 flag (incomplete reduction)
            "jnz 2b",
            "fstp qword ptr [{out}]",
            "fstp st(0)",
            x = in(reg) &x as *const f64,
            y = in(reg) &y as *const f64,
            out = in(reg) &mut result as *mut f64,
            out("ax") _,
        );
    }
    result
}

#[no_mangle]
pub extern "C" fn log(x: f64) -> f64 {
    // ln(x) = log2(x) * ln(2)
    // fyl2x computes y * log2(x), so we use y = ln(2) = fldln2
    let mut result: f64 = 0.0;
    unsafe {
        asm!(
            "fldln2",
            "fld qword ptr [{x}]",
            "fyl2x",
            "fstp qword ptr [{out}]",
            x = in(reg) &x as *const f64,
            out = in(reg) &mut result as *mut f64,
        );
    }
    result
}

#[no_mangle]
pub extern "C" fn log10(x: f64) -> f64 {
    let mut result: f64 = 0.0;
    unsafe {
        asm!(
            "fldlg2",
            "fld qword ptr [{x}]",
            "fyl2x",
            "fstp qword ptr [{out}]",
            x = in(reg) &x as *const f64,
            out = in(reg) &mut result as *mut f64,
        );
    }
    result
}

#[no_mangle]
pub extern "C" fn log2(x: f64) -> f64 {
    let mut result: f64 = 0.0;
    unsafe {
        asm!(
            "fld1",
            "fld qword ptr [{x}]",
            "fyl2x",
            "fstp qword ptr [{out}]",
            x = in(reg) &x as *const f64,
            out = in(reg) &mut result as *mut f64,
        );
    }
    result
}

#[no_mangle]
pub extern "C" fn exp(x: f64) -> f64 {
    // e^x = 2^(x * log2(e))
    // f2xm1 computes 2^x - 1 for |x| <= 1
    // For larger values we need to split into integer and fraction
    let mut result: f64 = 0.0;
    unsafe {
        asm!(
            "fld qword ptr [{x}]",
            "fldl2e",           // st0 = log2(e), st1 = x
            "fmulp",            // st0 = x * log2(e)
            // Split into integer and fraction
            "fld st(0)",        // duplicate
            "frndint",          // st0 = int part, st1 = x*log2e
            "fsub st(1), st(0)",// st1 = frac part
            "fxch",             // st0 = frac, st1 = int
            "f2xm1",           // st0 = 2^frac - 1
            "fld1",
            "faddp",           // st0 = 2^frac
            "fscale",          // st0 = 2^frac * 2^int = 2^(x*log2e) = e^x
            "fstp st(1)",      // clean up
            "fstp qword ptr [{out}]",
            x = in(reg) &x as *const f64,
            out = in(reg) &mut result as *mut f64,
        );
    }
    result
}

#[no_mangle]
pub extern "C" fn pow(base: f64, exponent: f64) -> f64 {
    if base == 0.0 {
        return 0.0;
    }
    if exponent == 0.0 {
        return 1.0;
    }
    // base^exp = 2^(exp * log2(base))
    exp(exponent * log2(base) * core::f64::consts::LN_2)
}

#[no_mangle]
pub extern "C" fn ldexp(x: f64, n: i32) -> f64 {
    // x * 2^n
    let mut result: f64 = 0.0;
    let n_f64 = n as f64;
    unsafe {
        asm!(
            "fld qword ptr [{n}]",
            "fld qword ptr [{x}]",
            "fscale",
            "fstp st(1)",
            "fstp qword ptr [{out}]",
            x = in(reg) &x as *const f64,
            n = in(reg) &n_f64 as *const f64,
            out = in(reg) &mut result as *mut f64,
        );
    }
    result
}

#[no_mangle]
pub extern "C" fn frexp(x: f64, exp: *mut i32) -> f64 {
    if x == 0.0 {
        unsafe { *exp = 0; }
        return 0.0;
    }
    // Extract exponent using bit manipulation
    let bits = x.to_bits();
    let biased_exp = ((bits >> 52) & 0x7FF) as i32;
    unsafe { *exp = biased_exp - 1022; }
    // Return mantissa with exponent = -1 (0.5 <= |result| < 1.0)
    f64::from_bits((bits & 0x800FFFFFFFFFFFFF) | 0x3FE0000000000000)
}

// Float versions
#[no_mangle]
pub extern "C" fn sinf(x: f32) -> f32 {
    sin(x as f64) as f32
}

#[no_mangle]
pub extern "C" fn cosf(x: f32) -> f32 {
    cos(x as f64) as f32
}

#[no_mangle]
pub extern "C" fn sqrtf(x: f32) -> f32 {
    sqrt(x as f64) as f32
}

#[no_mangle]
pub extern "C" fn tanf(x: f32) -> f32 {
    tan(x as f64) as f32
}

#[no_mangle]
pub extern "C" fn atan2f(y: f32, x: f32) -> f32 {
    atan2(y as f64, x as f64) as f32
}

#[no_mangle]
pub extern "C" fn floorf(x: f32) -> f32 {
    floor(x as f64) as f32
}

#[no_mangle]
pub extern "C" fn ceilf(x: f32) -> f32 {
    ceil(x as f64) as f32
}

#[no_mangle]
pub extern "C" fn powf(base: f32, exp: f32) -> f32 {
    pow(base as f64, exp as f64) as f32
}

#[no_mangle]
pub extern "C" fn logf(x: f32) -> f32 {
    log(x as f64) as f32
}

#[no_mangle]
pub extern "C" fn expf(x: f32) -> f32 {
    exp(x as f64) as f32
}

#[no_mangle]
pub extern "C" fn fmodf(x: f32, y: f32) -> f32 {
    fmod(x as f64, y as f64) as f32
}

#[no_mangle]
pub extern "C" fn asin(x: f64) -> f64 {
    // asin(x) = atan2(x, sqrt(1 - x*x))
    atan2(x, sqrt(1.0 - x * x))
}

#[no_mangle]
pub extern "C" fn acos(x: f64) -> f64 {
    // acos(x) = atan2(sqrt(1 - x*x), x)
    atan2(sqrt(1.0 - x * x), x)
}
