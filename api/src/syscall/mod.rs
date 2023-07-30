pub mod exec;

use core::arch::asm;

pub fn syscall(a: u32, b: u32, c: u32, d: u32) -> u32 {
    let result: u32;
    unsafe {
        asm!(
            "int 0x2b",
            inout("eax") a => result,
            in("ebx") b,
            in("ecx") c,
            in("edx") d,
        );
    }
    result
}
