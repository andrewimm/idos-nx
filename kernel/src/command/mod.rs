use alloc::string::String;

use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::files::FileHandle;
use crate::task::actions::io::{read_file, write_file, open_path, transfer_handle};

fn command_task() -> ! {
    let stdin = FileHandle::new(0);
    let stdout = FileHandle::new(1);

    let mut input_buffer: [u8; 256] = [0; 256];

    let mut prompt = String::from("\nA:\\>");

    loop {
        write_file(stdout, prompt.as_bytes()).unwrap();
        let input_len = read_file(stdin, &mut input_buffer).unwrap() as usize;

        write_file(stdout, "\nGOT: ".as_bytes()).unwrap();
        write_file(stdout, &input_buffer[..input_len]).unwrap();
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
