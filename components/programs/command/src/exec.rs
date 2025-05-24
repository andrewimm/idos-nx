use crate::{
    env::Environment,
    parser::{CommandComponent, CommandTree},
};

use alloc::string::String;
use alloc::vec::Vec;

use idos_api::io::error::IOError;
use idos_api::io::sync::{close_sync, open_sync, read_sync, write_sync};
use idos_api::syscall::io::create_file_handle;

pub fn exec_command_tree(env: &mut Environment, tree: CommandTree) {
    let root = match tree.get_root() {
        Some(component) => component,
        None => return,
    };

    match root {
        CommandComponent::Executable(name, args) => match name.to_ascii_uppercase().as_str() {
            "CD" | "CHDIR" => cd(env, args),
            "DIR" => dir(env, args),
            "TYPE" => type_file(env, args),
            _ => {
                if is_drive(name.as_bytes()) {
                    let mut cd_args = Vec::new();
                    cd_args.push(String::from(name));
                    cd(env, &cd_args);
                } else {
                    let _ = write_sync(env.stdout, "Unknown command!\n".as_bytes(), 0);
                }
            }
        },
        _ => {
            let _ = write_sync(env.stdout, "Unsupported syntax!\n".as_bytes(), 0);
        }
    }
}

fn is_drive(name: &[u8]) -> bool {
    for i in 0..(name.len() - 1) {
        if name[i] < b'A' {
            return false;
        }
        if name[i] > b'Z' && name[i] < b'a' {
            return false;
        }
        if name[i] > b'z' {
            return false;
        }
    }
    if name[name.len() - 1] != b':' {
        return false;
    }
    true
}

fn cd(env: &mut Environment, args: &Vec<String>) {
    let change_to = args.get(0).cloned();
    match change_to {
        Some(ref arg) => {
            if arg.starts_with("\\") {
                // absolute path
            } else if is_drive(arg.as_bytes()) {
                // drive switch
            } else {
                // relative path
                let mut split_iter = arg.split("\\");
                loop {
                    match split_iter.next() {
                        Some(chunk) => match chunk {
                            "." => (),
                            ".." => env.popd(),
                            dir => env.pushd(dir.as_bytes()),
                        },
                        None => break,
                    }
                }
            }
        }
        None => {
            // no argument, change to root
            env.pop_to_root();
        }
    }
}

fn dir(env: &Environment, args: &Vec<String>) {}

fn type_file(env: &Environment, args: &Vec<String>) {
    if args.is_empty() {
        let _ = write_sync(env.stdout, "No file specified!\n".as_bytes(), 0);
        return;
    }
    for arg in args {
        type_file_inner(env, arg);
    }
}

fn type_file_inner(env: &Environment, arg: &String) -> Result<(), IOError> {
    let handle = create_file_handle();
    // TODO: handle relative and absolute paths
    let _ = open_sync(handle, arg.as_str())?;
    let mut read_offset = 0;

    let _ = close_sync(handle)?;
    Ok(())
}
