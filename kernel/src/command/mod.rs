use alloc::string::String;

use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::files::FileHandle;
use crate::task::actions::io::{read_file, write_file, open_path, transfer_handle, get_current_drive_name, set_active_drive, close_file};

fn command_task() -> ! {
    let stdin = FileHandle::new(0);
    let stdout = FileHandle::new(1);

    set_active_drive("DEV");

    let mut input_buffer: [u8; 256] = [0; 256];
    let mut file_read_buffer: [u8; 256] = [0; 256];

    let mut prompt = get_current_drive_name();
    prompt.push_str(":\\");

    loop {
        write_file(stdout, prompt.as_bytes()).unwrap();
        let input_len = read_file(stdin, &mut input_buffer).unwrap() as usize;

        let input_str = unsafe { core::str::from_utf8_unchecked(&input_buffer[..input_len]).trim() };

        if input_str == "dir" {
            let mut output = String::from("Directory of ");
            output.push_str("DEV:\\\n\n");
            write_file(stdout, output.as_bytes()).unwrap();

            let dir_handle = open_path("DEV:\\").unwrap();
            loop {
                let bytes_read = read_file(dir_handle, &mut file_read_buffer).unwrap() as usize;
                for i in 0..bytes_read {
                    if file_read_buffer[i] == 0 {
                        file_read_buffer[i] = b'\n';
                    }
                }
                write_file(stdout, &file_read_buffer[..bytes_read]).unwrap();
                if bytes_read < file_read_buffer.len() {
                    break;
                }
            }
            close_file(dir_handle).unwrap();
        } else if input_str == "cd" {
            write_file(stdout, "Change dir\n".as_bytes()).unwrap();
        }
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
