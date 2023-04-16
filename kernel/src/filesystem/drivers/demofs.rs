use crate::filesystem::drivers::asyncfs::AsyncCommand;
use crate::task::actions::{read_message_blocking, send_message};
use crate::task::messaging::Message;

use super::asyncfs::{ASYNC_RESPONSE_MAGIC, AsyncDriver};

struct DemoFS {}

impl AsyncDriver for DemoFS {
    fn open(&mut self, path: &str) -> u32 {
        crate::kprint!("  Err, you want me to open \"{}\"?\n", path);
        let handle = 1;
        handle
    }

    fn read(&mut self, buffer: &mut [u8]) -> u32 {
        buffer[0] = b'A';
        buffer[1] = b'B';
        buffer[2] = b'C';
        let written = 3;
        written
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

fn create_response(a: u32, b: u32, c: u32) -> Message {
    Message(ASYNC_RESPONSE_MAGIC, a, b, c)
}
