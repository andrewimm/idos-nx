use crate::task::actions::{read_message_blocking, send_message};

use super::asyncfs::AsyncDriver;

struct DemoFS {}

impl AsyncDriver for DemoFS {
    fn open(&mut self, path: &str) -> u32 {
        crate::kprint!("  Err, you want me to open \"{}\"?\n", path);
        let handle = 1;
        handle
    }

    fn read(&mut self, _handle: u32, buffer: &mut [u8]) -> u32 {
        buffer[0] = b'A';
        buffer[1] = b'B';
        buffer[2] = b'C';
        let written = 3;
        written
    }

    fn write(&mut self, _handle: u32, _buffer: &[u8]) -> u32 {
        0
    }

    fn close(&mut self, _handle: u32) {
        
    }

    fn seek(&mut self, _instance: u32, _offset: crate::files::cursor::SeekMethod) -> u32 {
        0
    }
}

pub fn demo_fs_task() -> ! {
    let mut driver_impl = DemoFS {};
    loop {
        let (message_read, _) = read_message_blocking(None);
        if let Some(next_message) = message_read {
            let (sender, message) = next_message.open();
            
            // do work in here
            crate::kprint!("  DEMO FS DO YOUR STUFF\n");
            
            match driver_impl.handle_request(message) {
                Some(response) => send_message(sender, response, 0xffffffff),
                None => continue,
            }
        }
    }
}

