#![no_std]
#![no_main]

extern crate alloc;
extern crate idos_api;
extern crate idos_sdk;

use idos_api::io::handle::Handle;
use idos_api::io::sync::{read_sync, write_sync};

mod env;
mod lexer;
mod parser;

#[no_mangle]
pub extern "C" fn main() {
    let stdin = Handle::new(0);
    let stdout = Handle::new(1);

    let mut input_buffer: [u8; 256] = [0; 256];
    let mut prompt: [u8; 256] = [0; 256];
    let mut prompt_len = 0;

    let mut env = self::env::Environment::new("C:");

    loop {
        idos_sdk::allocator::reset();

        prompt_len = env.put_cwd(&mut prompt);
        prompt[prompt_len] = b'>';
        prompt_len += 1;

        let _ = write_sync(stdout, &prompt[..prompt_len], 0);
        match read_sync(stdin, &mut input_buffer, 0) {
            Ok(read_len) => {
                let lexer = self::lexer::Lexer::new(&input_buffer[..(read_len as usize)]);
                let mut parser = self::parser::Parser::new(lexer);
                parser.parse_input();
                let tree = parser.into_tree();

                let root = match tree.get_root() {
                    Some(component) => component,
                    None => return,
                };

                match root {
                    self::parser::CommandComponent::Executable(name, args) => {
                        let output = alloc::format!("Got: \"{}\"\n", name);
                        let _ = write_sync(stdout, output.as_bytes(), 0);
                    }
                    _ => (),
                }
            }
            Err(_) => (),
        }
    }
}
