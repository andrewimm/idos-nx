use core::sync::atomic::{AtomicU32, Ordering};

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::time::date::DateTime;
use crate::time::system::Timestamp;
use crate::{task::{files::FileHandle, actions::{io::{write_file, open_path, read_file, close_file, set_active_drive, get_current_drive_name, get_current_dir, file_stat, dup_handle, transfer_handle}, memory::map_memory}, memory::MemoryBacking}, files::path::Path, filesystem::install_device_driver};

use super::parser::{CommandTree, CommandComponent};
use super::Environment;

static IO_BUFFERS: AtomicU32 = AtomicU32::new(0);

pub fn init_buffers() {
    let addr = map_memory(None, 0x1000, MemoryBacking::Anonymous).unwrap();
    unsafe {
        core::ptr::write_volatile(addr.as_ptr_mut::<u8>(), 0);
    }
    IO_BUFFERS.store(addr.as_u32(), Ordering::SeqCst);
}

pub fn get_buffers() -> &'static mut [u8] {
    let ptr = IO_BUFFERS.load(Ordering::SeqCst) as *mut u8;
    let len = 0x1000;

    unsafe {
        core::slice::from_raw_parts_mut(ptr, len)
    }
}

pub fn exec(stdin: FileHandle, stdout: FileHandle, tree: CommandTree, env: &mut Environment) {
    let root = match tree.get_root() {
        Some(component) => component,
        None => return,
    };

    match root {
        CommandComponent::Executable(name, args) => {
            let mut output = String::new();

            write_file(stdout, output.as_bytes()).unwrap();
            match name.to_ascii_uppercase().as_str() {
                "CD" => cd(stdout, args, env),
                "DIR" => dir(stdout, args, env),
                "DRIVES" => drives(stdout),
                "MKDEV" => install_device(args),
                "TYPE" => type_file(stdout, args),
                _ => {
                    if Path::is_drive(name) {
                        let mut cd_args = Vec::new();
                        cd_args.push(String::from(name));
                        cd(stdout, &cd_args, env);
                    } else if try_exec(stdin, stdout, name, args, env) {

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

struct DirEntry {
    name: String,
    size: u32,
    mod_timestamp: u32,
}

fn dir(stdout: FileHandle, args: &Vec<String>, env: &Environment) {
    let file_read_buffer = get_buffers();

    let mut output = String::from(" Volume in drive is UNKNOWN\n Volume Serial Number is UNKNOWN\n Directory of ");
    output.push_str(&env.drive);
    output.push_str(":\\");
    output.push_str(env.cwd.as_str());
    output.push_str("\n\n");
    write_file(stdout, output.as_bytes()).unwrap();

    let dir_handle = open_path(env.cwd.as_str()).unwrap();
    let mut entries: Vec<DirEntry> = Vec::new();
    loop {
        let bytes_read = read_file(dir_handle, file_read_buffer).unwrap() as usize;
        let mut name_start = 0;
        for i in 0..bytes_read {
            if file_read_buffer[i] == 0 {
                let name = String::from_utf8_lossy(&file_read_buffer[name_start..i]);
                entries.push(DirEntry { name: name.to_string(), size: 0, mod_timestamp: 0 });
                name_start = i + 1;
            }
        }
        //write_file(stdout, &file_read_buffer[..bytes_read]).unwrap();
        if bytes_read < file_read_buffer.len() {
            break;
        }
    }
    close_file(dir_handle).unwrap();
    for entry in entries.iter_mut() {
        match open_path(&entry.name) {
            Ok(handle) => {
                match file_stat(handle) {
                    Ok(stat) => {
                        entry.size = stat.byte_size;
                        entry.mod_timestamp = stat.modification_time;
                    },
                    Err(_) => (),
                }
                close_file(handle);
            },
            Err(_) => (),
        }
    }
    for entry in entries.iter() {
        let mut row = String::from("");
        row.push_str(&entry.name);
        for _ in entry.name.len()..13 {
            row.push(' ');
        }
        row.push_str(&alloc::format!("{:>9} ", entry.size));
        let datetime = DateTime::from_timestamp(Timestamp(entry.mod_timestamp));
        row.push_str(&datetime.date.to_string());
        row.push(' ');
        row.push_str(&datetime.time.to_string());
        row.push('\n');

        write_file(stdout, row.as_bytes()).unwrap();
    }

    let mut summary = String::new();
    for _ in 0..13 {
        summary.push(' ');
    }
    summary.push_str(&alloc::format!("{} file(s)\n", entries.len()));
    write_file(stdout, summary.as_bytes()).unwrap();
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

fn try_exec(stdin: FileHandle, stdout: FileHandle, name: &str, args: &Vec<String>, env: &Environment) -> bool {
    match open_path(name) {
        Ok(handle) => {
            close_file(handle).unwrap();
        },
        Err(_) => return false,
    }

    let exec_child = crate::task::actions::lifecycle::create_task();
    crate::task::actions::lifecycle::add_args(exec_child, args);
    crate::task::actions::lifecycle::attach_executable_to_task(exec_child, name);

    let stdin_dup = dup_handle(stdin).unwrap();
    let stdout_dup = dup_handle(stdout).unwrap();

    transfer_handle(stdin_dup, exec_child).unwrap();
    transfer_handle(stdout_dup, exec_child).unwrap();

    crate::task::actions::lifecycle::wait_for_child(exec_child, None);

    true
}

fn type_file(stdout: FileHandle, args: &Vec<String>) {
    let buffer = get_buffers();
    if args.is_empty() {
        return;
    }
    for arg in args {
        match open_path(arg) {
            Ok(handle) => {
                loop {
                    let len = read_file(handle, buffer).unwrap() as usize;
                    write_file(stdout, &buffer[..len]).unwrap();

                    if len < buffer.len() {
                        break;
                    }
                }
                write_file(stdout, &[b'\n']).unwrap();
                close_file(handle).unwrap();
            },
            Err(_) => {
                let output = alloc::format!("File not found: \"{}\"\n", arg);
                write_file(stdout, output.as_bytes()).unwrap();
                return;
            },
        }
    }
}

fn install_device(args: &Vec<String>) {
    if args.len() < 2 {
        return;
    }

    let name = args.get(0).unwrap();
    let mount = args.get(1).unwrap();

    match open_path(name) {
        Ok(handle) => {
            close_file(handle).unwrap();
        },
        Err(_) => return,
    }

    let driver_task = crate::task::actions::lifecycle::create_task();
    crate::task::actions::lifecycle::attach_executable_to_task(driver_task, name);

    install_device_driver(&mount.to_ascii_uppercase(), driver_task, 0);
}

