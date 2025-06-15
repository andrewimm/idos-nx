use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};

use crate::io::handle::Handle;

pub fn _kprint(args: fmt::Arguments) {
    let mut serial = crate::hardware::com::serial::SerialPort::new(0x3f8);
    serial.write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => ($crate::log::_kprint(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! kprintln {
    () => ($crate::kprint!("\n"));
    ($($arg:tt)*) => ($crate::kprint!("{}\n", format_args!($($arg)*)));
}

pub struct Logger {
    log_lines: Vec<String>,
}

impl Logger {
    pub fn new() -> Self {
        Logger {
            log_lines: Vec::new(),
        }
    }

    pub fn log(&mut self, message: &str) {
        use alloc::string::ToString;
        self.log_lines.push(message.to_string());
        kprintln!("LOG: {}", message);
    }

    pub fn flush_to_file(&mut self, handle: Handle) {
        for line in &self.log_lines {
            let _ = crate::task::actions::io::write_sync(handle, line.as_bytes(), 0);
        }
        self.log_lines.clear();
    }
}
