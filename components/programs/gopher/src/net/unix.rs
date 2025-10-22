use super::NetworkProvider;

pub struct PlatformNetProvider {}

impl NetworkProvider for PlatformNetProvider {
    fn new() -> Self {
        Self {
        }
    }

    fn connect(&self, _address: &str, _port: u16) -> bool {
        true
    }

    fn send(&self, _data: &[u8]) -> usize {
        0
    }

    fn receive(&self, _buffer: &mut [u8]) -> usize {
        0
    }
}
