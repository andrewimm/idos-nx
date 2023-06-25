use alloc::string::String;
use alloc::vec::Vec;

use crate::{task::{files::FileHandle, actions::io::{write_file, open_path, read_file, close_file, set_active_drive, get_current_drive_name, get_current_dir, file_stat}}, files::path::Path};

use super::parser::{CommandTree, CommandComponent};
use super::Environment;

pub fn exec(stdout: FileHandle, tree: CommandTree, env: &mut Environment) {
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
                "CD" => cd(stdout, args, env),
                "DIR" => dir(stdout, args, env),
                "DRIVES" => drives(stdout),
                _ => {
                    if Path::is_drive(name) {
                        let mut cd_args = Vec::new();
                        cd_args.push(String::from(name));
                        cd(stdout, &cd_args, env);
                    } else if try_exec(stdout, name, args, env) {

                    } else {
                        write_file(stdout, "Unknown command!\n".as_bytes()).unwrap();
                    }
                },
            }
        },
        _ => panic!("unimplemented"),
    }
}

fn cd(stdout: FileHandle, args: &Vec<String>, env: &mut Environment) {
    let change_to = args.get(0).cloned().unwrap_or(String::from("/"));
    let path = if let Some((drive, path)) = Path::split_absolute_path(change_to.as_str()) {
        match set_active_drive(drive) {
            Ok(_) => crate::kprint!("CHange active drive\n"),
            Err(_) => {
                write_file(stdout, "No such drive\n".as_bytes()).unwrap();
            },
        }
        path
    } else {
        change_to.as_str()
    };

    //set_current_dir(path)
    env.drive = get_current_drive_name();
    env.cwd = get_current_dir();
}

fn dir(stdout: FileHandle, args: &Vec<String>, env: &Environment) {
    let mut file_read_buffer: [u8; 256] = [0; 256];

    let mut output = String::from("Directory of ");
    output.push_str(&env.drive);
    output.push_str(":\\");
    output.push_str(env.cwd.as_str());
    output.push_str("\n\n");
    write_file(stdout, output.as_bytes()).unwrap();

    let dir_handle = open_path(env.cwd.as_str()).unwrap();
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

fn drives(stdout: FileHandle) {
    let mut output = String::new();
    let mut names = crate::filesystem::get_drive_names();
    names.sort();
    for name in names {
        output.push_str(&name);
        output.push('\n');
    }
    write_file(stdout, output.as_bytes()).unwrap();
}

fn try_exec(stdout: FileHandle, name: &str, args: &Vec<String>, env: &Environment) -> bool {
    match open_path(name) {
        Ok(handle) => {
            close_file(handle).unwrap();
        },
        Err(_) => return false,
    }

    let exec_child = crate::task::actions::lifecycle::create_task();
    crate::task::actions::lifecycle::attach_executable_to_task(exec_child, name);
    crate::task::actions::lifecycle::wait_for_child(exec_child, None);

    true
}

