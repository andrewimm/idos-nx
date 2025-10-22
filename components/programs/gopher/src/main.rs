mod config;
mod net;
mod terminal;

use net::NetworkProvider;
use terminal::TerminalControl;

fn main() {
    let mut term_control = terminal::PlatformTerminalControl::new();
    term_control.set_raw_mode();

    term_control.write("Gopher!\n".as_bytes());

    let mut net_control = net::PlatformNetProvider::new();

    let mut read_buffer: [u8; 4] = [0; 4];
    loop {
        let bytes_read = term_control.read(&mut read_buffer);
        if bytes_read > 0 {
            let available_bytes = &read_buffer[0..bytes_read];

            if available_bytes[0] == b'q' {
                break;
            }
        }
    }
}
