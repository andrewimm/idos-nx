use crate::task::actions::{read_message_blocking, send_message};
use crate::task::messaging::Message;

use super::asyncfs::ASYNC_RESPONSE_MAGIC;


pub fn demo_fs_task() -> ! {
    loop {
        let (message_read, _) = read_message_blocking(None);
        if let Some(next_message) = message_read {
            let (sender, message) = next_message.open();
            
            // do work in here
            crate::kprint!("  DEMO FS DO YOUR STUFF\n");

            let response = Message(ASYNC_RESPONSE_MAGIC, 0, 0, 0);
            send_message(sender, response, 0xffffffff);
        }
    }
}
