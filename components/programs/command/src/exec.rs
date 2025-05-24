use crate::{
    env::Environment,
    parser::{CommandComponent, CommandTree},
};

use core::sync::atomic::{AtomicPtr, Ordering};

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use idos_api::io::error::IOError;
use idos_api::io::sync::{close_sync, open_sync, read_sync, write_sync};
use idos_api::syscall::io::create_file_handle;
use idos_api::syscall::memory::map_memory;

static IO_BUFFER: AtomicPtr<u8> = AtomicPtr::new(0xffff_ffff as *mut u8);

fn get_io_buffer() -> &'static mut [u8] {
    let buffer_start = {
        let stored = IO_BUFFER.load(Ordering::SeqCst);
        if stored as u32 == 0xffff_ffff {
            let addr = map_memory(None, 0x1000, None).unwrap();
            unsafe {
                // force a page fault to assign memory
                core::ptr::write_volatile(addr as *mut u8, 0);
            }
            IO_BUFFER.store(addr as *mut u8, Ordering::SeqCst);
            addr as *mut u8
        } else {
            stored
        }
    };
    unsafe { core::slice::from_raw_parts_mut(buffer_start, 0x1000) }
}

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
                env.reset_drive(arg.as_bytes());
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

struct DirEntry {
    name: String,
    size: u32,
    mod_timestamp: u32,
    is_dir: bool,
}

fn dir(env: &Environment, args: &Vec<String>) {
    let file_read_buffer = get_io_buffer();

    let mut output = String::from(
        " Volume in drive is UNKNOWN\n Volume Serial Number is UNKNOWN\n Directory of ",
    );
    output.push_str(env.cwd_string());
    output.push_str("\n\n");
    let _ = write_sync(env.stdout, output.as_bytes(), 0);

    let dir_handle = create_file_handle();
    match open_sync(dir_handle, env.cwd_string()) {
        Ok(_) => (),
        Err(_) => {
            let _ = write_sync(env.stdout, "Failed to open directory...\n".as_bytes(), 0);
            return;
        }
    }
    let mut entries: Vec<DirEntry> = Vec::new();
    let mut read_offset = 0;
    loop {
        let bytes_read = read_sync(dir_handle, file_read_buffer, read_offset).unwrap() as usize;
        read_offset += bytes_read as u32;
        let mut name_start = 0;
        for i in 0..bytes_read {
            if file_read_buffer[i] == 0 {
                let name = String::from_utf8_lossy(&file_read_buffer[name_start..i]);
                entries.push(DirEntry {
                    name: name.to_string(),
                    size: 0,
                    mod_timestamp: 0,
                    is_dir: false,
                });
                name_start = i + 1;
            }
        }
        if bytes_read < file_read_buffer.len() {
            break;
        }
    }
    let _ = close_sync(dir_handle);

    for entry in entries.iter() {
        let mut row = String::from("");
        row.push_str(&entry.name);
        for _ in entry.name.len()..13 {
            row.push(' ');
        }
        if entry.is_dir {
            row.push_str("<DIR>     ");
        } else {
            row.push_str(&alloc::format!("{:>9} ", entry.size));
        }
        //let datetime = DateTime::from_timestamp(Timestamp(entry.mod_timestamp));
        //row.push_str(&datetime.date.to_string());
        //row.push(' ');
        //row.push_str(&datetime.time.to_string());
        row.push('\n');

        let _ = write_sync(env.stdout, row.as_bytes(), 0);
    }

    let mut summary = String::new();
    for _ in 0..13 {
        summary.push(' ');
    }
    summary.push_str(&alloc::format!("{} file(s)\n", entries.len()));
    let _ = write_sync(env.stdout, summary.as_bytes(), 0);
}

fn type_file(env: &Environment, args: &Vec<String>) {
    if args.is_empty() {
        let _ = write_sync(env.stdout, "No file specified!\n".as_bytes(), 0);
        return;
    }
    for arg in args {
        type_file_inner(env, arg);
    }
}

fn type_file_inner(env: &Environment, arg: &String) -> Result<(), ()> {
    let handle = create_file_handle();
    // TODO: handle relative and absolute paths
    let _ = open_sync(handle, arg.as_str()).map_err(|_| ());
    let mut read_offset = 0;

    let buffer = get_io_buffer();
    loop {
        let len = match read_sync(handle, buffer, read_offset) {
            Ok(len) => len as usize,
            Err(_) => {
                let _ = write_sync(env.stdout, "Error reading file\n".as_bytes(), 0);
                return Err(());
            }
        };
        read_offset += len as u32;
        let _ = write_sync(env.stdout, &buffer[..len], 0);

        if len < buffer.len() {
            break;
        }
    }

    let _ = close_sync(handle).map_err(|_| ())?;
    Ok(())
}
