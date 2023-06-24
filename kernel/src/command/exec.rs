use alloc::string::String;
use alloc::vec::Vec;

use crate::task::{files::FileHandle, actions::io::{write_file, open_path, read_file, close_file}};

use super::parser::{CommandTree, CommandComponent};

pub fn exec(stdout: FileHandle, tree: CommandTree) {
    let root = match tree.get_root() {
        Some(component) => component,
        None => return,
    };

    match root {
        CommandComponent::Executable(name, args) => {
            let mut output = alloc::format!("RUN \"{}\" with args: ", name);
            for arg in args {
                output.push_str(&arg);
                output.push_str(", ");
            }
            output.push('\n');

            write_file(stdout, output.as_bytes()).unwrap();
            match name.to_ascii_uppercase().as_str() {
                "CD" => cd(stdout, args),
                "DIR" => dir(stdout, args),
                _ => {
                    write_file(stdout, "Unknown command!\n".as_bytes()).unwrap();
                },
            }
        },
        _ => panic!("unimplemented"),
    }
}

fn cd(stdout: FileHandle, args: &Vec<String>) {
    let change_to = args.get(0).cloned().unwrap_or(String::from("/"));
    let output = alloc::format!("Change directory to {}\n", change_to);
    write_file(stdout, output.as_bytes()).unwrap();
}

fn dir(stdout: FileHandle, args: &Vec<String>) {
    let mut file_read_buffer: [u8; 256] = [0; 256];

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
}
