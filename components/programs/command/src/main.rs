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
    let mut prompt: [u8; 256] = [0; 256];
    let mut prompt_len = 0;

    let mut env = Environment::new("C:");

    loop {
        idos_sdk::allocator::reset();

        prompt_len = env.put_cwd(&mut prompt);
        prompt[prompt_len] = b'>';
        prompt_len += 1;

        let _ = write_sync(stdout, &prompt[..prompt_len], 0);
        match read_sync(stdin, &mut input_buffer, 0) {
            Ok(read_len) => {
                let mut len = read_len as usize;
                if input_buffer[len - 1] == b'\n' {
                    len -= 1;
                }
                env.pushd(&input_buffer[..len]);
            }
            Err(_) => (),
        }
    }
}

struct Environment {
    cwd: [u8; 256],
    cwd_length: usize,
}

impl Environment {
    pub fn new(drive: &str) -> Self {
        let mut cwd = [0; 256];
        let drive_bytes = drive.as_bytes();
        cwd[..drive_bytes.len()].copy_from_slice(drive_bytes);
        cwd[drive_bytes.len()] = b'\\';
        Self {
            cwd,
            cwd_length: drive_bytes.len() + 1,
        }
    }

    pub fn put_cwd(&mut self, buffer: &mut [u8]) -> usize {
        let mut i = 0;
        while i < self.cwd_length && i < buffer.len() {
            buffer[i] = self.cwd[i];
            i += 1;
        }
        self.cwd_length
    }

    pub fn cwd_bytes(&self) -> &[u8] {
        &self.cwd[..self.cwd_length]
    }

    pub fn pushd(&mut self, dir_bytes: &[u8]) {
        if self.cwd_length + dir_bytes.len() + 1 < self.cwd.len() {
            self.cwd[self.cwd_length..self.cwd_length + dir_bytes.len()].copy_from_slice(dir_bytes);
            self.cwd_length += dir_bytes.len();
            self.cwd[self.cwd_length] = b'\\';
            self.cwd_length += 1;
        }
    }
}
