use crate::filesystem::drivers::asyncfs::AsyncCommand;
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

            let response = match AsyncCommand::from(message.0) {
                AsyncCommand::Open => {
                    let handle = 1;
                    create_response(handle, 0, 0)
                },
                AsyncCommand::Read => {
                    let written = 3;
                    create_response(written, 0, 0)
                },
                _ => {
                    crate::kprint!("  DEMO FS GOT UNKNOWN COMMAND\n");
                    continue
                },
            };

            send_message(sender, response, 0xffffffff);
        }
    }
}

fn create_response(a: u32, b: u32, c: u32) -> Message {
    Message(ASYNC_RESPONSE_MAGIC, a, b, c)
}
