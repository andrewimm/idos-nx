#![no_std]
#![no_main]

extern crate alloc;
extern crate idos_api;
extern crate idos_sdk;

use idos_api::io::sync::{read_sync, write_sync};

mod env;
mod exec;
mod lexer;
mod parser;

#[no_mangle]
pub extern "C" fn main() {
    let mut input_buffer: [u8; 256] = [0; 256];
    let mut prompt: [u8; 256] = [0; 256];
    let mut prompt_len = 0;

    let mut env = self::env::Environment::new("C:");

    loop {
        idos_sdk::allocator::reset();

        prompt_len = env.expand_prompt(&mut prompt);

        let _ = write_sync(env.stdout, &prompt[..prompt_len], 0);
        match read_sync(env.stdin, &mut input_buffer, 0) {
            Ok(read_len) => {
                let lexer = self::lexer::Lexer::new(&input_buffer[..(read_len as usize)]);
                let mut parser = self::parser::Parser::new(lexer);
                parser.parse_input();
                let tree = parser.into_tree();

                self::exec::exec_command_tree(&mut env, tree);
            }
            Err(_) => (),
        }
    }
}
