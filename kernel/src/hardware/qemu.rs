use crate::arch::port::Port;

pub fn debug_exit(code: u32) -> ! {
    // QEMU Debug Exit port on ISA bus
    // The value of `code` will be shifted one bit to the left, and OR-ed with
    // 1 to create an exit value for the qemu process.
    Port::new(0xf4).write_u32(code);
    unreachable!();
}
