use core::sync::atomic::{AtomicU32, Ordering};

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::files::path::Path;
use crate::files::stat::FileStatus;
use crate::io::async_io::FILE_OP_STAT;
use crate::io::filesystem::get_all_drive_names;
use crate::io::handle::{Handle, PendingHandleOp};
use crate::task::actions::handle::{
    create_file_handle, handle_op_close, handle_op_open, handle_op_read, handle_op_write,
    set_active_drive,
};
use crate::task::actions::memory::map_memory;
use crate::task::memory::MemoryBacking;
use crate::time::date::DateTime;
use crate::time::system::Timestamp;

use super::parser::{CommandComponent, CommandTree};
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

    unsafe { core::slice::from_raw_parts_mut(ptr, len) }
}

pub fn exec(stdin: Handle, stdout: Handle, tree: CommandTree, env: &mut Environment) {
    let root = match tree.get_root() {
        Some(component) => component,
        None => return,
    };

    match root {
        CommandComponent::Executable(name, args) => {
            let mut output = String::new();

            handle_op_write(stdout, output.as_bytes());
            match name.to_ascii_uppercase().as_str() {
                "CD" => cd(stdout, args, env),
                "DIR" => dir(stdout, args, env),
                "DRIVES" => drives(stdout),
                //"MKDEV" => install_device(args),
                "TYPE" => type_file(stdout, args, env),
                _ => {
                    if Path::is_drive(name) {
                        let mut cd_args = Vec::new();
                        cd_args.push(String::from(name));
                        cd(stdout, &cd_args, env);
                    } else if try_exec(stdin, stdout, name, args, env) {
                    } else {
                        handle_op_write(stdout, "Unknown command!\n".as_bytes());
                    }
                }
            }
        }
        _ => panic!("unimplemented"),
    }
}

fn cd(stdout: Handle, args: &Vec<String>, env: &mut Environment) {
    let change_to = args.get(0).cloned().unwrap_or(String::from("/"));
    if Path::is_absolute(change_to.as_str()) || Path::is_drive(change_to.as_str()) {
        env.cwd = Path::from_str(change_to.as_str());
    } else {
        crate::kprintln!("NOT ABS {}", change_to);
    }
}

struct DirEntry {
    name: String,
    size: u32,
    mod_timestamp: u32,
    is_dir: bool,
}

fn dir(stdout: Handle, args: &Vec<String>, env: &Environment) {
    let file_read_buffer = get_buffers();

    let mut output = String::from(
        " Volume in drive is UNKNOWN\n Volume Serial Number is UNKNOWN\n Directory of ",
    );
    output.push_str(env.cwd.as_str());
    output.push_str("\n\n");
    handle_op_write(stdout, output.as_bytes());

    let dir_handle = create_file_handle();
    handle_op_open(dir_handle, env.cwd.as_str()).wait_for_completion();
    let mut entries: Vec<DirEntry> = Vec::new();
    let mut read_offset = 0;
    loop {
        let bytes_read = handle_op_read(dir_handle, file_read_buffer, read_offset)
            .wait_for_completion() as usize;
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
    handle_op_close(dir_handle);
    for entry in entries.iter_mut() {
        let stat_handle = create_file_handle();
        let mut file_status = FileStatus::new();
        let file_status_ptr = &mut file_status as *mut FileStatus;
        let mut file_path = env.cwd.clone();
        file_path.push(entry.name.as_str());
        match handle_op_open(stat_handle, file_path.as_str()).wait_for_result() {
            Ok(_) => {
                let op = PendingHandleOp::new(
                    stat_handle,
                    FILE_OP_STAT,
                    file_status_ptr as u32,
                    core::mem::size_of::<FileStatus>() as u32,
                    0,
                );
                match op.wait_for_result() {
                    Ok(_) => {
                        entry.size = file_status.byte_size;
                        entry.mod_timestamp = file_status.modification_time;
                        entry.is_dir = file_status.file_type & 2 != 0;
                    }
                    Err(_) => (),
                }
                handle_op_close(stat_handle).wait_for_completion();
            }
            Err(_) => {}
        }
    }
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
        let datetime = DateTime::from_timestamp(Timestamp(entry.mod_timestamp));
        row.push_str(&datetime.date.to_string());
        row.push(' ');
        row.push_str(&datetime.time.to_string());
        row.push('\n');

        handle_op_write(stdout, row.as_bytes());
    }

    let mut summary = String::new();
    for _ in 0..13 {
        summary.push(' ');
    }
    summary.push_str(&alloc::format!("{} file(s)\n", entries.len()));
    handle_op_write(stdout, summary.as_bytes());
}

fn drives(stdout: Handle) {
    let mut output = String::new();
    let mut names = get_all_drive_names();
    names.sort();
    for name in names {
        output.push_str(&name);
        output.push('\n');
    }
    handle_op_write(stdout, output.as_bytes());
}

fn try_exec(
    stdin: Handle,
    stdout: Handle,
    name: &str,
    args: &Vec<String>,
    env: &Environment,
) -> bool {
    /*
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
    */
    true
}

fn type_file(stdout: Handle, args: &Vec<String>, env: &Environment) {
    let buffer = get_buffers();
    if args.is_empty() {
        return;
    }
    for arg in args {
        let handle = create_file_handle();
        crate::kprintln!("TYPE {}", arg);
        let file_path = if Path::is_absolute(arg.as_str()) {
            Path::from_str(arg.as_str())
        } else {
            let mut path = env.cwd.clone();
            path.push(arg.as_str());
            path
        };
        match handle_op_open(handle, file_path.as_str()).wait_for_result() {
            Ok(_) => {
                let mut read_offset = 0;
                loop {
                    let len = match handle_op_read(handle, buffer, read_offset).wait_for_result() {
                        Ok(len) => len as usize,
                        Err(_) => {
                            handle_op_write(stdout, "Error reading file\n".as_bytes());
                            return;
                        }
                    };
                    read_offset += len as u32;
                    handle_op_write(stdout, &buffer[..len]).wait_for_completion();

                    if len < buffer.len() {
                        break;
                    }
                }
                handle_op_write(stdout, &[b'\n']);
                handle_op_close(handle);
            }
            Err(_) => {
                let output = alloc::format!("File not found: \"{}\"\n", arg);
                handle_op_write(stdout, output.as_bytes());
                return;
            }
        }
    }
}

/*
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
*/
