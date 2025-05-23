#![no_std]
#![no_main]

extern crate idos_api;
extern crate idos_sdk;

use idos_api::io::handle::Handle;
use idos_api::io::sync::{read_sync, write_sync};

#[no_mangle]
pub extern "C" fn main() {
    let stdin = Handle::new(0);
    let stdout = Handle::new(1);

    let mut input_buffer: [u8; 256] = [0; 256];

    loop {
        let prompt = "> ";
        let _ = write_sync(stdout, prompt.as_bytes(), 0);
        match read_sync(stdin, &mut input_buffer, 0) {
            Ok(len) => {}
            Err(_) => (),
        }
    }
}
