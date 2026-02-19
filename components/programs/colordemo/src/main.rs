#![no_std]
#![no_main]

extern crate idos_api;
extern crate idos_sdk;

use idos_api::io::{sync::write_sync, Handle};
use idos_api::syscall::memory::map_memory;

const STDOUT: Handle = Handle::new(1);
const PAGE_SIZE: usize = 0x1000;

struct Buf {
    ptr: *mut u8,
    len: usize,
}

impl Buf {
    fn new() -> Self {
        let addr = map_memory(None, PAGE_SIZE as u32, None).unwrap();
        Self { ptr: addr as *mut u8, len: 0 }
    }

    fn data(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }

    fn push(&mut self, s: &[u8]) {
        let end = (self.len + s.len()).min(PAGE_SIZE);
        let count = end - self.len;
        unsafe {
            core::ptr::copy_nonoverlapping(s.as_ptr(), self.ptr.add(self.len), count);
        }
        self.len = end;
    }

    fn push_byte(&mut self, b: u8) {
        if self.len < PAGE_SIZE {
            unsafe { *self.ptr.add(self.len) = b; }
            self.len += 1;
        }
    }

    fn flush(&self) {
        let _ = write_sync(STDOUT, self.data(), 0);
    }
}

#[no_mangle]
pub extern "C" fn main() {
    let mut buf = Buf::new();

    // Clear screen
    buf.push(b"\x1b[2J\x1b[H");

    // Title
    buf.push(b"\x1b[1;97m=== ANSI Color Demo ===\x1b[0m\n\n");

    // Normal foreground colors
    buf.push(b"  Normal FG:  ");
    for i in 0u8..8 {
        buf.push(b"\x1b[3");
        buf.push_byte(b'0' + i);
        buf.push(b"m## ");
    }
    buf.push(b"\x1b[0m\n");

    // Bright foreground colors
    buf.push(b"  Bright FG:  ");
    for i in 0u8..8 {
        buf.push(b"\x1b[9");
        buf.push_byte(b'0' + i);
        buf.push(b"m## ");
    }
    buf.push(b"\x1b[0m\n");

    // Color names
    buf.push(b"              ");
    let names: [&[u8]; 8] = [
        b"BK ", b"RD ", b"GN ", b"YL ",
        b"BL ", b"MG ", b"CN ", b"WH ",
    ];
    for name in &names {
        buf.push(name);
    }
    buf.push(b"\n\n");

    // Background color blocks
    buf.push(b"  Normal BG:  ");
    for i in 0u8..8 {
        buf.push(b"\x1b[4");
        buf.push_byte(b'0' + i);
        buf.push(b"m  \x1b[0m ");
    }
    buf.push(b"\n");

    buf.push(b"  Bright BG:  ");
    for i in 0u8..8 {
        buf.push(b"\x1b[10");
        buf.push_byte(b'0' + i);
        buf.push(b"m  \x1b[0m ");
    }
    buf.push(b"\n\n");

    // Attribute demos
    buf.push(b"  \x1b[1mBold/Bright\x1b[0m  ");
    buf.push(b"\x1b[7mReverse\x1b[0m  ");
    buf.push(b"\x1b[1;31mBold Red\x1b[0m  ");
    buf.push(b"\x1b[1;32mBold Green\x1b[0m\n\n");

    // Color matrix: FG on BG combinations
    buf.push(b"  FG\\BG ");
    for bg in 0u8..8 {
        buf.push(b"\x1b[4");
        buf.push_byte(b'0' + bg);
        buf.push(b"m   ");
    }
    buf.push(b"\x1b[0m\n");

    for fg in 0u8..8 {
        // Row label
        buf.push(b"  \x1b[3");
        buf.push_byte(b'0' + fg);
        buf.push(b"m##\x1b[0m  ");

        // Grid cells
        for bg in 0u8..8 {
            buf.push(b"\x1b[3");
            buf.push_byte(b'0' + fg);
            buf.push(b";4");
            buf.push_byte(b'0' + bg);
            buf.push(b"maB ");
        }
        buf.push(b"\x1b[0m\n");
    }
    buf.push(b"\n");

    // Rainbow gradient banner
    buf.push(b"  ");
    let rainbow: [&[u8]; 6] = [
        b"\x1b[91m", b"\x1b[93m", b"\x1b[92m",
        b"\x1b[96m", b"\x1b[94m", b"\x1b[95m",
    ];
    let banner = b"* I D O S - N X *";
    for (i, &ch) in banner.iter().enumerate() {
        buf.push(rainbow[i % 6]);
        buf.push_byte(ch);
    }
    buf.push(b"\x1b[0m\n\n");

    buf.flush();
}
