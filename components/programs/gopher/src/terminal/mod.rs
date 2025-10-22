mod unix;

pub use self::unix::PlatformTerminalControl;

pub trait TerminalControl {
    fn new() -> Self;

    fn set_raw_mode(&mut self);
    fn restore(&mut self);

    fn read(&self, buffer: &mut [u8]) -> usize;
    fn write(&self, buffer: &[u8]) -> usize;
}

