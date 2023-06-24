pub mod exec;
pub mod lexer;
pub mod parser;

use alloc::string::String;

use crate::files::path::Path;
use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::files::FileHandle;
use crate::task::actions::io::{read_file, write_file, open_path, transfer_handle, get_current_drive_name, set_active_drive, close_file, get_current_dir};

use self::lexer::Lexer;
use self::parser::Parser;

fn command_task() -> ! {
    let stdin = FileHandle::new(0);
    let stdout = FileHandle::new(1);

    set_active_drive("DEV");

    let mut input_buffer: [u8; 256] = [0; 256];

    let mut env = Environment {
        drive: get_current_drive_name(),
        cwd: get_current_dir(),
    };

    loop {
        let mut prompt = env.full_path_string();
        prompt.push_str("> ");

        write_file(stdout, prompt.as_bytes()).unwrap();
        let input_len = read_file(stdin, &mut input_buffer).unwrap() as usize;

        let input_str = unsafe { core::str::from_utf8_unchecked(&input_buffer[..input_len]).trim() };

        let mut lexer = Lexer::new(input_str);
        let mut parser = Parser::new(lexer);
        parser.parse_input();

        self::exec::exec(stdout, parser.into_tree(), &mut env);
    }
}

pub struct Environment {
    pub drive: String,
    pub cwd: Path,
}

impl Environment {
    pub fn full_path_string(&self) -> String {
        alloc::format!("{}:\\{}", self.drive, self.cwd.as_str())
    }
}

pub fn start_command(console: usize) {
    let path = alloc::format!("DEV:\\CON{}", console + 1);

    let stdin = open_path(path.as_str()).unwrap();
    let stdout = open_path(path.as_str()).unwrap();
    let task_id = create_kernel_task(command_task);
    transfer_handle(stdin, task_id);
    transfer_handle(stdout, task_id);
}
