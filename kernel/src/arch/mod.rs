pub mod gdt;
pub mod port;
pub mod segment;

use core::arch::asm;

pub fn rdtsc() -> (u32, u32) {
    let mut low: u32;
    let mut high: u32;
    unsafe {
        asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
        );
    }
    (high, low)
}
