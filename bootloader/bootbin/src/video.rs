use core::arch::asm;
use core::fmt::Write;

pub fn print_char(ch: u8) {
    let ax = (ch as u16) | 0x0e00;
    unsafe {
        asm!(
            "push bx",
            "mov bx, 0",
            "int 0x10",
            "pop bx",
            in("ax") ax,
        );
    }
}

pub fn print_string(s: &str) {
    for ch in s.bytes() {
        print_char(ch);
    }
}

pub struct VideoWriter;

impl Write for VideoWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        print_string(s);
        Ok(())
    }
}

