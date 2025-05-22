#![no_std]
#![no_main]

extern crate idos_api;
extern crate idos_sdk;

use idos_api::io::handle::Handle;
use idos_api::io::sync::write_sync;

#[no_mangle]
pub extern "C" fn main() {
    let stdout = Handle::new(1);
    let _ = write_sync(stdout, "HELLO\n".as_bytes(), 0);
}
