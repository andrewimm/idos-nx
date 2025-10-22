mod unix;

pub use self::unix::PlatformNetProvider;

pub trait NetworkProvider {
    fn new() -> Self;
    fn connect(&self, address: &str, port: u16) -> bool;
    fn send(&self, data: &[u8]) -> usize;
    fn receive(&self, buffer: &mut [u8]) -> usize;
}
