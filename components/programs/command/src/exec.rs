use crate::{
    env::Environment,
    parser::{CommandComponent, CommandTree},
};

use core::sync::atomic::{AtomicPtr, Ordering};

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use idos_api::io::{
    handle::dup_handle,
    sync::{close_sync, open_sync, read_sync, share_sync, write_sync},
};
use idos_api::syscall::io::create_file_handle;
use idos_api::syscall::memory::map_memory;
use idos_api::time::DateTime;
use idos_api::{io::file::FileStatus, syscall::exec::create_task};
use idos_api::{
    io::{error::IoError, sync::io_sync, FILE_OP_STAT},
    syscall::exec::load_executable,
};

static IO_BUFFER: AtomicPtr<u8> = AtomicPtr::new(0xffff_ffff as *mut u8);

pub fn get_io_buffer() -> &'static mut [u8] {
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

/// Parse and execute a single line of input (used by batch file executor).
pub fn exec_line(env: &mut Environment, line: &[u8]) {
    let lexer = crate::lexer::Lexer::new(line);
    let mut parser = crate::parser::Parser::new(lexer);
    parser.parse_input();
    let tree = parser.into_tree();
    exec_command_tree(env, tree);
}

pub fn exec_command_tree(env: &mut Environment, tree: CommandTree) {
    let root = match tree.get_root() {
        Some(component) => component,
        None => return,
    };

    match root {
        CommandComponent::Executable(name, args) => match name.to_ascii_uppercase().as_str() {
            "CD" | "CHDIR" => cd(env, args),
            "CLS" => cls(env),
            "COLOR" => color(env, args),
            "DIR" => dir(env, args),
            "ECHO" => echo(env, args),
            "PROMPT" => prompt(env, args),
            //"DRIVES" => drives(env),
            "TYPE" => type_file(env, args),
            "VER" => ver(env),
            _ => {
                if is_drive(name.as_bytes()) {
                    let mut cd_args = Vec::new();
                    cd_args.push(String::from(name));
                    cd(env, &cd_args);
                } else if !try_external(env, name, args) {
                    let _ = write_sync(env.stdout, "Unknown command!\n".as_bytes(), 0);
                }
            }
        },
        _ => {
            let _ = write_sync(env.stdout, "Unsupported syntax!\n".as_bytes(), 0);
        }
    }
}

fn echo(env: &Environment, args: &Vec<String>) {
    // ECHO with no args could toggle echo state, but for now just print a blank line
    if args.is_empty() {
        let _ = write_sync(env.stdout, b"\n", 0);
        return;
    }
    // Rejoin args with spaces
    let mut buf = [0u8; 256];
    let mut len = 0;
    for (i, arg) in args.iter().enumerate() {
        if i > 0 && len < buf.len() {
            buf[len] = b' ';
            len += 1;
        }
        let bytes = arg.as_bytes();
        let n = bytes.len().min(buf.len() - len);
        buf[len..len + n].copy_from_slice(&bytes[..n]);
        len += n;
    }
    if len < buf.len() {
        buf[len] = b'\n';
        len += 1;
    }
    let _ = write_sync(env.stdout, &buf[..len], 0);
}

fn ver(env: &Environment) {
    let _ = write_sync(env.stdout, b"\nIDOS-NX Version 0.1\n\n", 0);
}

fn cls(env: &Environment) {
    // ESC[2J clears the screen, ESC[H moves cursor to top-left
    let _ = write_sync(env.stdout, b"\x1b[2J\x1b[H", 0);
}

fn color(env: &Environment, args: &Vec<String>) {
    if args.is_empty() {
        // Reset to defaults (light gray on black)
        let _ = write_sync(env.stdout, b"\x1b[0m\x1b[2J\x1b[H", 0);
        return;
    }

    let arg = args[0].as_bytes();
    if arg.len() != 2 {
        let _ = write_sync(env.stdout, b"Usage: COLOR [bg_fg]\n  Two hex digits (0-F): background, foreground\n  Example: COLOR 0A (green on black)\n", 0);
        return;
    }

    let bg = match hex_digit(arg[0]) {
        Some(v) => v,
        None => {
            let _ = write_sync(env.stdout, b"Invalid hex digit\n", 0);
            return;
        }
    };
    let fg = match hex_digit(arg[1]) {
        Some(v) => v,
        None => {
            let _ = write_sync(env.stdout, b"Invalid hex digit\n", 0);
            return;
        }
    };

    if fg == bg {
        let _ = write_sync(env.stdout, b"Foreground and background cannot be the same\n", 0);
        return;
    }

    // Map CGA color index to ANSI SGR codes
    // CGA indices 0-7 map to normal, 8-15 map to bright
    let mut buf = [0u8; 32];
    let mut len = 0;

    // Reset first
    buf[len..len + 4].copy_from_slice(b"\x1b[0m");
    len += 4;

    // Foreground
    len += write_sgr_fg(&mut buf[len..], fg);

    // Background
    len += write_sgr_bg(&mut buf[len..], bg);

    // Clear screen with new colors
    buf[len..len + 7].copy_from_slice(b"\x1b[2J\x1b[H");
    len += 7;

    let _ = write_sync(env.stdout, &buf[..len], 0);
}

/// CGA index to ANSI color code offset. CGA and ANSI have different orderings
/// for blue/red, cyan/yellow, etc.
const CGA_TO_ANSI: [u8; 8] = [0, 4, 2, 6, 1, 5, 3, 7];

fn write_sgr_fg(buf: &mut [u8], cga: u8) -> usize {
    let base = CGA_TO_ANSI[(cga & 7) as usize];
    if cga >= 8 {
        // Bright: ESC[9Xm
        buf[0..2].copy_from_slice(b"\x1b[");
        buf[2] = b'9';
        buf[3] = b'0' + base;
        buf[4] = b'm';
        5
    } else {
        // Normal: ESC[3Xm
        buf[0..2].copy_from_slice(b"\x1b[");
        buf[2] = b'3';
        buf[3] = b'0' + base;
        buf[4] = b'm';
        5
    }
}

fn write_sgr_bg(buf: &mut [u8], cga: u8) -> usize {
    let base = CGA_TO_ANSI[(cga & 7) as usize];
    if cga >= 8 {
        // Bright: ESC[10Xm
        buf[0..3].copy_from_slice(b"\x1b[1");
        buf[3] = b'0';
        buf[4] = b'0' + base;
        buf[5] = b'm';
        6
    } else {
        // Normal: ESC[4Xm
        buf[0..2].copy_from_slice(b"\x1b[");
        buf[2] = b'4';
        buf[3] = b'0' + base;
        buf[4] = b'm';
        5
    }
}

fn prompt(env: &mut Environment, args: &Vec<String>) {
    if args.is_empty() {
        // No argument resets to default
        env.set_prompt(b"$P$G");
        return;
    }
    // Rejoin args with spaces to reconstruct the format string
    let mut buf = [0u8; 128];
    let mut len = 0;
    for (i, arg) in args.iter().enumerate() {
        if i > 0 && len < buf.len() {
            buf[len] = b' ';
            len += 1;
        }
        let bytes = arg.as_bytes();
        let n = bytes.len().min(buf.len() - len);
        buf[len..len + n].copy_from_slice(&bytes[..n]);
        len += n;
    }
    env.set_prompt(&buf[..len]);
}

fn hex_digit(ch: u8) -> Option<u8> {
    match ch {
        b'0'..=b'9' => Some(ch - b'0'),
        b'A'..=b'F' => Some(ch - b'A' + 10),
        b'a'..=b'f' => Some(ch - b'a' + 10),
        _ => None,
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

    for entry in entries.iter_mut() {
        let stat_handle = create_file_handle();
        let mut file_status = FileStatus::new();
        let file_status_ptr = &mut file_status as *mut FileStatus;
        let mut file_path = String::from(env.cwd_string());
        file_path.push_str(entry.name.as_str());
        match open_sync(stat_handle, file_path.as_str()) {
            Ok(_) => {
                let op = io_sync(
                    stat_handle,
                    FILE_OP_STAT,
                    file_status_ptr as u32,
                    core::mem::size_of::<FileStatus>() as u32,
                    0,
                );
                entry.size = file_status.byte_size;
                entry.mod_timestamp = file_status.modification_time;
                entry.is_dir = file_status.file_type & 2 != 0;
                let _ = close_sync(stat_handle);
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
        let datetime = DateTime::from_timestamp(entry.mod_timestamp);
        let day = datetime.date.day;
        let month = datetime.date.month;
        let year = datetime.date.year;
        row.push_str(&alloc::format!("{:02}-{:02}-{:04}", day, month, year));
        row.push(' ');
        let hours = datetime.time.hours;
        let minutes = datetime.time.minutes;
        let seconds = datetime.time.seconds;
        row.push_str(&alloc::format!(
            "{:02}:{:02}:{:02}",
            hours,
            minutes,
            seconds,
        ));
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
    let file_path = env.full_file_path(arg);
    let _ = open_sync(handle, file_path.as_str()).map_err(|_| ());
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

/// Try to resolve a command name to an external file. Checks in order:
/// exact name, name.ELF, name.BAT. Dispatches to the appropriate executor.
fn try_external(env: &mut Environment, name: &String, args: &Vec<String>) -> bool {
    // If the name already has an extension, try it directly
    if has_extension(name.as_bytes()) {
        let path = env.full_file_path(name);
        if ends_with_ignore_case(path.as_bytes(), b".BAT") {
            if file_exists(&path) {
                crate::batch::exec_batch(env, path.as_str(), args);
                return true;
            }
        } else {
            return try_exec(env, &path, args);
        }
        return false;
    }

    // Try name.ELF
    let mut elf_name = name.clone();
    elf_name.push_str(".ELF");
    let elf_path = env.full_file_path(&elf_name);
    if try_exec(env, &elf_path, args) {
        return true;
    }

    // Try name.BAT
    let mut bat_name = name.clone();
    bat_name.push_str(".BAT");
    let bat_path = env.full_file_path(&bat_name);
    if file_exists(&bat_path) {
        crate::batch::exec_batch(env, bat_path.as_str(), args);
        return true;
    }

    // Try exact name as-is (maybe it has no extension but is an ELF)
    let exact_path = env.full_file_path(name);
    try_exec(env, &exact_path, args)
}

fn has_extension(name: &[u8]) -> bool {
    name.iter().any(|&c| c == b'.')
}

fn ends_with_ignore_case(s: &[u8], suffix: &[u8]) -> bool {
    if s.len() < suffix.len() {
        return false;
    }
    let start = s.len() - suffix.len();
    for i in 0..suffix.len() {
        let a = if s[start + i].is_ascii_alphabetic() { s[start + i] | 0x20 } else { s[start + i] };
        let b = if suffix[i].is_ascii_alphabetic() { suffix[i] | 0x20 } else { suffix[i] };
        if a != b {
            return false;
        }
    }
    true
}

fn file_exists(path: &str) -> bool {
    let handle = create_file_handle();
    match open_sync(handle, path) {
        Ok(_) => {
            let _ = close_sync(handle);
            true
        }
        Err(_) => false,
    }
}

fn try_exec(env: &Environment, exec_path: &str, args: &Vec<String>) -> bool {
    let exec_handle = create_file_handle();
    match open_sync(exec_handle, exec_path) {
        Ok(_) => {
            let _ = close_sync(exec_handle);
        }
        Err(_) => return false,
    }
    let (child_handle, child_id) = create_task();

    // Build arg structure: argv[0] = program path, then any additional args
    // Format: [u16 len][bytes][u16 len][bytes]...
    let arg_structure_size: usize =
        exec_path.len() + 2 + args.iter().map(|s| s.len() + 2).sum::<usize>();
    let mut arg_structure_buffer = Vec::with_capacity(arg_structure_size);
    // argv[0] = program path
    let len_low = (exec_path.len() & 0xFF) as u8;
    let len_high = ((exec_path.len() >> 8) & 0xFF) as u8;
    arg_structure_buffer.push(len_low);
    arg_structure_buffer.push(len_high);
    arg_structure_buffer.extend_from_slice(exec_path.as_bytes());
    // argv[1..] = additional args
    for arg in args {
        let len_low = (arg.len() & 0xFF) as u8;
        let len_high = ((arg.len() >> 8) & 0xFF) as u8;
        arg_structure_buffer.push(len_low);
        arg_structure_buffer.push(len_high);
        arg_structure_buffer.extend_from_slice(arg.as_bytes());
    }
    idos_api::syscall::exec::add_args(
        child_id,
        arg_structure_buffer.as_ptr(),
        arg_structure_size as u32,
    );

    let stdin_dup = dup_handle(env.stdin).unwrap();
    let stdout_dup = dup_handle(env.stdout).unwrap();

    // Share handles BEFORE load_executable, because load_executable makes
    // the child runnable immediately. If we share after, the child may start
    // running before its stdin/stdout handles exist (race condition).
    share_sync(stdin_dup, child_id).unwrap();
    share_sync(stdout_dup, child_id).unwrap();

    if !load_executable(child_id, exec_path) {
        // exec failed — clean up the handles we created
        // TODO: the shares already completed, would need to revoke them
        let _ = close_sync(child_handle);
        return false;
    }

    let _ = read_sync(child_handle, &mut [0u8], 0);
    true
}
