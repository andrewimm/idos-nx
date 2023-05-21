use crate::task::actions::read_message_blocking;

struct AtaDeviceDriver {
}

impl AtaDeviceDriver {
    pub fn new() -> Self {
        

        Self {
        }
    }
}

fn run_driver() -> ! {
    crate::kprint!("Install ATA device driver\n");

    let mut driver_impl = AtaDeviceDriver::new();

    loop {
        let (message_read, _) = read_message_blocking(None);
        if let Some(packet) = message_read {
            let (sender, message) = packet.open();

            //match driver_impl.handle_request(message) {
            //    Some(response) => send_message(sender, response, 0xffffffff),
            //    None => continue,
            //}
        }

    }
}
