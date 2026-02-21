pub mod exec;
pub mod lexer;
pub mod parser;

use alloc::string::String;

use crate::files::path::Path;
use crate::io::handle::Handle;
use crate::task::actions::handle::{create_file_handle, transfer_handle};
use crate::task::actions::io::{open_sync, read_sync, write_sync};
use crate::task::actions::lifecycle::create_kernel_task;

use self::lexer::Lexer;
use self::parser::Parser;

fn command_task() -> ! {
    let stdin = Handle::new(0);
    let stdout = Handle::new(1);

    let mut input_buffer: [u8; 256] = [0; 256];

    let mut env = Environment {
        cwd: Path::from_str("DEV:\\"),
    };

    self::exec::init_buffers();

    loop {
        let mut prompt = env.full_path_string();
        prompt.push_str("> ");

        write_sync(stdout, prompt.as_bytes(), 0).unwrap();
        let input_len = read_sync(stdin, &mut input_buffer, 0).unwrap() as usize;

        let input_str =
            unsafe { core::str::from_utf8_unchecked(&input_buffer[..input_len]).trim() };

        let lexer = Lexer::new(input_str);
        let mut parser = Parser::new(lexer);
        parser.parse_input();

        self::exec::exec(stdin, stdout, parser.into_tree(), &mut env);
    }
}

pub struct Environment {
    pub cwd: Path,
}

impl Environment {
    pub fn full_path_string(&self) -> String {
        Into::<String>::into(self.cwd.clone())
    }
}

pub fn start_command(console: usize) {
    let path = alloc::format!("DEV:\\CON{}", console + 1);

    let stdin = create_file_handle();
    open_sync(stdin, path.as_str(), 0).unwrap();
    let stdout = create_file_handle();
    open_sync(stdout, path.as_str(), 0).unwrap();
    let task_id = create_kernel_task(command_task, Some("COMMAND"));
    transfer_handle(stdin, task_id);
    transfer_handle(stdout, task_id);
}
