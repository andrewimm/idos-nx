use core::fmt::{self, Write};

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

