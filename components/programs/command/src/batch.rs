use alloc::string::String;
use alloc::vec::Vec;

use idos_api::io::sync::{close_sync, open_sync, read_sync, write_sync};
use idos_api::syscall::io::create_file_handle;

use crate::env::Environment;
use crate::exec::{exec_line, get_io_buffer};

/// Execute a .BAT batch file. Arguments are available as %0-%9 within the file.
pub fn exec_batch(env: &mut Environment, path: &str, args: &Vec<String>) {
    let handle = create_file_handle();
    match open_sync(handle, path, 0) {
        Ok(_) => {}
        Err(_) => {
            let _ = write_sync(env.stdout, b"Failed to open batch file\n", 0);
            return;
        }
    }

    // Read entire file into the shared IO buffer (up to 4K page)
    let file_buf = get_io_buffer();
    let mut total = 0;
    loop {
        let len = match read_sync(handle, &mut file_buf[total..], total as u32) {
            Ok(n) => n as usize,
            Err(_) => break,
        };
        total += len;
        if len == 0 || total >= file_buf.len() {
            break;
        }
    }
    let _ = close_sync(handle);

    // Copy file data to a Vec so the IO buffer is free for commands to use
    let mut file_data = Vec::with_capacity(total);
    file_data.extend_from_slice(&file_buf[..total]);

    let mut pos = 0;
    let mut goto_target: Option<([u8; 128], usize)> = None;

    while pos < file_data.len() {
        // Find end of line
        let line_start = pos;
        while pos < file_data.len() && file_data[pos] != b'\n' {
            pos += 1;
        }
        let mut line_end = pos;
        if pos < file_data.len() {
            pos += 1;
        }
        if line_end > line_start && file_data[line_end - 1] == b'\r' {
            line_end -= 1;
        }

        let line = &file_data[line_start..line_end];
        let trimmed = trim_leading_whitespace(line);
        if trimmed.is_empty() {
            continue;
        }

        // If searching for a GOTO label, only check labels
        if let Some((ref target, tlen)) = goto_target {
            if trimmed[0] == b':' {
                let label = trim_leading_whitespace(&trimmed[1..]);
                if ascii_eq_ignore_case(label, &target[..tlen]) {
                    goto_target = None;
                }
            }
            continue;
        }

        // Labels: skip during normal execution
        if trimmed[0] == b':' {
            continue;
        }

        // REM: skip comments
        if starts_with_keyword(trimmed, b"REM") {
            continue;
        }

        // Perform %0-%9 parameter substitution
        let mut expanded = [0u8; 512];
        let expanded_len = substitute_params(trimmed, args, path, &mut expanded);
        let expanded_line = &expanded[..expanded_len];

        // Check for GOTO before dispatching
        if starts_with_keyword(expanded_line, b"GOTO") {
            let rest = trim_leading_whitespace(&expanded_line[4..]);
            if !rest.is_empty() {
                let mut target = [0u8; 128];
                let len = rest.len().min(128);
                target[..len].copy_from_slice(&rest[..len]);
                goto_target = Some((target, len));
                pos = 0; // restart from top of file
            }
            continue;
        }

        exec_line(env, expanded_line);
    }
}

fn trim_leading_whitespace(s: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < s.len() && (s[i] == b' ' || s[i] == b'\t') {
        i += 1;
    }
    &s[i..]
}

/// Check if `line` starts with a case-insensitive keyword followed by
/// end-of-line or whitespace.
fn starts_with_keyword(line: &[u8], keyword: &[u8]) -> bool {
    if line.len() < keyword.len() {
        return false;
    }
    for i in 0..keyword.len() {
        let a = if line[i].is_ascii_alphabetic() { line[i] | 0x20 } else { line[i] };
        let b = if keyword[i].is_ascii_alphabetic() { keyword[i] | 0x20 } else { keyword[i] };
        if a != b {
            return false;
        }
    }
    line.len() == keyword.len() || line[keyword.len()] == b' ' || line[keyword.len()] == b'\t'
}

fn ascii_eq_ignore_case(a: &[u8], b: &[u8]) -> bool {
    // Trim trailing whitespace from a for label matching
    let a = {
        let mut end = a.len();
        while end > 0 && (a[end - 1] == b' ' || a[end - 1] == b'\t') {
            end -= 1;
        }
        &a[..end]
    };
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        let ca = if a[i].is_ascii_alphabetic() { a[i] | 0x20 } else { a[i] };
        let cb = if b[i].is_ascii_alphabetic() { b[i] | 0x20 } else { b[i] };
        if ca != cb {
            return false;
        }
    }
    true
}

/// Replace %0-%9 with batch arguments. %0 = batch file path, %1-%9 = args.
fn substitute_params(line: &[u8], args: &Vec<String>, path: &str, out: &mut [u8]) -> usize {
    let mut r = 0;
    let mut w = 0;
    while r < line.len() && w < out.len() {
        if line[r] == b'%' && r + 1 < line.len() && line[r + 1] >= b'0' && line[r + 1] <= b'9' {
            let idx = (line[r + 1] - b'0') as usize;
            let replacement: &[u8] = if idx == 0 {
                path.as_bytes()
            } else if idx - 1 < args.len() {
                args[idx - 1].as_bytes()
            } else {
                b""
            };
            let n = replacement.len().min(out.len() - w);
            out[w..w + n].copy_from_slice(&replacement[..n]);
            w += n;
            r += 2;
        } else {
            out[w] = line[r];
            w += 1;
            r += 1;
        }
    }
    w
}
