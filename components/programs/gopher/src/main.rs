#![no_std]
#![no_main]

extern crate idos_api;
extern crate idos_sdk;

use idos_api::{
    io::{
        sync::{read_sync, write_sync, open_sync},
        Handle,
    },
    syscall::net::create_tcp_handle,
};

const STDIN: Handle = Handle::new(0);
const STDOUT: Handle = Handle::new(1);
const GOPHER_PORT: u16 = 70;
const MAX_LINKS: usize = 64;

struct Link {
    item_type: u8,
    selector: [u8; 128],
    selector_len: usize,
    host: [u8; 64],
    host_len: usize,
    port: u16,
}

struct History {
    entries: [[u8; 256]; 8],
    lengths: [usize; 8],
    hosts: [[u8; 64]; 8],
    host_lengths: [usize; 8],
    ports: [u16; 8],
    depth: usize,
}

impl History {
    fn new() -> Self {
        Self {
            entries: [[0; 256]; 8],
            lengths: [0; 8],
            hosts: [[0; 64]; 8],
            host_lengths: [0; 8],
            ports: [0; 8],
            depth: 0,
        }
    }

    fn push(&mut self, host: &[u8], selector: &[u8], port: u16) {
        if self.depth < 8 {
            let d = self.depth;
            let slen = selector.len().min(256);
            self.entries[d][..slen].copy_from_slice(&selector[..slen]);
            self.lengths[d] = slen;
            let hlen = host.len().min(64);
            self.hosts[d][..hlen].copy_from_slice(&host[..hlen]);
            self.host_lengths[d] = hlen;
            self.ports[d] = port;
            self.depth += 1;
        }
    }

    fn pop(&mut self) -> Option<(&[u8], &[u8], u16)> {
        if self.depth > 0 {
            self.depth -= 1;
            let d = self.depth;
            Some((
                &self.hosts[d][..self.host_lengths[d]],
                &self.entries[d][..self.lengths[d]],
                self.ports[d],
            ))
        } else {
            None
        }
    }
}

#[no_mangle]
pub extern "C" fn main() {
    let mut args = idos_sdk::env::args();
    args.next(); // skip argv[0]

    let host = match args.next() {
        Some(h) => h,
        None => {
            let _ = write_sync(STDOUT, b"Usage: gopher <host> [selector]\n", 0);
            return;
        }
    };
    let selector = args.next().unwrap_or("");

    let mut current_host = [0u8; 64];
    let current_host_len = host.len().min(64);
    current_host[..current_host_len].copy_from_slice(&host.as_bytes()[..current_host_len]);

    let mut current_selector = [0u8; 256];
    let current_selector_len = selector.len().min(256);
    current_selector[..current_selector_len].copy_from_slice(&selector.as_bytes()[..current_selector_len]);

    let mut history = History::new();
    let mut links: [Link; MAX_LINKS] = core::array::from_fn(|_| Link {
        item_type: 0,
        selector: [0; 128],
        selector_len: 0,
        host: [0; 64],
        host_len: 0,
        port: 0,
    });

    let mut host_len = current_host_len;
    let mut sel_len = current_selector_len;

    loop {
        let num_links = fetch_and_display(
            &current_host[..host_len],
            &current_selector[..sel_len],
            GOPHER_PORT,
            &mut links,
        );

        if num_links == 0 {
            // Text page or error — just show prompt
        }

        let _ = write_sync(STDOUT, b"\n[#/b/q] > ", 0);

        let mut input_buf = [0u8; 32];
        let input_len = match read_sync(STDIN, &mut input_buf, 0) {
            Ok(n) => n as usize,
            Err(_) => break,
        };
        if input_len == 0 {
            continue;
        }

        // Trim trailing newline/CR
        let mut end = input_len;
        while end > 0 && (input_buf[end - 1] == b'\n' || input_buf[end - 1] == b'\r') {
            end -= 1;
        }
        if end == 0 {
            continue;
        }
        let input = &input_buf[..end];

        if input == b"q" {
            break;
        }
        if input == b"b" {
            match history.pop() {
                Some((h, s, _p)) => {
                    host_len = h.len().min(64);
                    current_host[..host_len].copy_from_slice(&h[..host_len]);
                    sel_len = s.len().min(256);
                    current_selector[..sel_len].copy_from_slice(&s[..sel_len]);
                }
                None => {
                    let _ = write_sync(STDOUT, b"No history\n", 0);
                    continue;
                }
            }
            continue;
        }

        // Try to parse as a link number
        if let Some(num) = parse_usize(input) {
            if num == 0 || num > num_links {
                let _ = write_sync(STDOUT, b"Invalid link number\n", 0);
                continue;
            }
            let link = &links[num - 1];

            // Save current page to history
            history.push(
                &current_host[..host_len],
                &current_selector[..sel_len],
                GOPHER_PORT,
            );

            // Navigate to the link (keep current host if link host is "0.0.0.0" or "0")
            let link_host = &link.host[..link.host_len];
            if link_host != b"0.0.0.0" && link_host != b"0" && !link_host.is_empty() {
                host_len = link.host_len.min(64);
                current_host[..host_len].copy_from_slice(&link.host[..host_len]);
            }
            sel_len = link.selector_len.min(256);
            current_selector[..sel_len].copy_from_slice(&link.selector[..sel_len]);
        } else {
            let _ = write_sync(STDOUT, b"Enter a link number, 'b' for back, or 'q' to quit\n", 0);
        }
    }
}

/// Connect to a gopher server, send the selector, read the response,
/// display it, and return the list of navigable links found.
fn fetch_and_display(
    host: &[u8],
    selector: &[u8],
    port: u16,
    links: &mut [Link; MAX_LINKS],
) -> usize {
    let host_str = unsafe { core::str::from_utf8_unchecked(host) };

    let _ = write_sync(STDOUT, b"\x1b[2J\x1b[H", 0); // clear screen
    let _ = write_sync(STDOUT, b"Connecting to ", 0);
    let _ = write_sync(STDOUT, host, 0);
    let _ = write_sync(STDOUT, b"...\n", 0);

    let socket = create_tcp_handle();
    if open_sync(socket, host_str, port as u32).is_err() {
        let _ = write_sync(STDOUT, b"Connection failed\n", 0);
        return 0;
    }

    // Send selector + \r\n
    let mut request = [0u8; 260];
    let mut req_len = selector.len().min(256);
    request[..req_len].copy_from_slice(&selector[..req_len]);
    request[req_len] = b'\r';
    request[req_len + 1] = b'\n';
    req_len += 2;

    if write_sync(socket, &request[..req_len], 0).is_err() {
        let _ = write_sync(STDOUT, b"Send failed\n", 0);
        return 0;
    }

    // Read the full response
    let mut response = [0u8; 16384];
    let mut total = 0;
    loop {
        let remaining = response.len() - total;
        if remaining == 0 {
            break;
        }
        match read_sync(socket, &mut response[total..], 0) {
            Ok(0) => break,
            Ok(n) => total += n as usize,
            Err(_) => break,
        }
    }

    if total == 0 {
        let _ = write_sync(STDOUT, b"Empty response\n", 0);
        return 0;
    }

    let _ = write_sync(STDOUT, b"\x1b[2J\x1b[H", 0); // clear screen

    // Try to detect if this is a gopher menu (lines starting with type char + tab-separated fields)
    let is_menu = detect_menu(&response[..total]);

    if is_menu {
        display_menu(&response[..total], links)
    } else {
        // Plain text — just display it
        let _ = write_sync(STDOUT, &response[..total], 0);
        0
    }
}

/// Heuristic: if the first line contains a tab character, it's probably a menu.
fn detect_menu(data: &[u8]) -> bool {
    for &b in data {
        if b == b'\t' {
            return true;
        }
        if b == b'\n' {
            return false;
        }
    }
    false
}

/// Display a gopher menu page. Each line is:
///   <type><display>\t<selector>\t<host>\t<port>\r\n
/// Returns the number of navigable links.
fn display_menu(data: &[u8], links: &mut [Link; MAX_LINKS]) -> usize {
    let mut link_count = 0;
    let mut pos = 0;

    while pos < data.len() {
        // Find end of line
        let line_start = pos;
        while pos < data.len() && data[pos] != b'\n' {
            pos += 1;
        }
        let mut line_end = pos;
        if line_end > line_start && data[line_end - 1] == b'\r' {
            line_end -= 1;
        }
        pos += 1; // skip \n

        let line = &data[line_start..line_end];
        if line.is_empty() || line == b"." {
            continue;
        }

        let item_type = line[0];
        let rest = &line[1..];

        // Split by tabs: display, selector, host, port
        let mut fields = [&[] as &[u8]; 4];
        let mut field_idx = 0;
        let mut field_start = 0;
        for i in 0..rest.len() {
            if rest[i] == b'\t' && field_idx < 3 {
                fields[field_idx] = &rest[field_start..i];
                field_idx += 1;
                field_start = i + 1;
            }
        }
        if field_idx < 4 {
            fields[field_idx] = &rest[field_start..];
        }

        let display_text = fields[0];

        let is_navigable = matches!(item_type, b'0' | b'1' | b'7');

        if is_navigable && link_count < MAX_LINKS {
            link_count += 1;

            let link = &mut links[link_count - 1];
            link.item_type = item_type;

            let sel = fields[1];
            link.selector_len = sel.len().min(128);
            link.selector[..link.selector_len].copy_from_slice(&sel[..link.selector_len]);

            let host = fields[2];
            link.host_len = host.len().min(64);
            link.host[..link.host_len].copy_from_slice(&host[..link.host_len]);

            link.port = parse_port(fields[3]).unwrap_or(70);

            // Print with link number
            let mut num_buf = [0u8; 8];
            let num_len = fmt_usize(&mut num_buf, link_count);

            let type_indicator = match item_type {
                b'0' => b"TXT",
                b'1' => b"DIR",
                b'7' => b"  ?",
                _ => b"   ",
            };

            let _ = write_sync(STDOUT, b" ", 0);
            let _ = write_sync(STDOUT, type_indicator, 0);
            let _ = write_sync(STDOUT, b" [", 0);
            let _ = write_sync(STDOUT, &num_buf[..num_len], 0);
            let _ = write_sync(STDOUT, b"] ", 0);
            let _ = write_sync(STDOUT, display_text, 0);
            let _ = write_sync(STDOUT, b"\n", 0);
        } else if item_type == b'i' {
            // Info line — just display text
            let _ = write_sync(STDOUT, b"      ", 0);
            let _ = write_sync(STDOUT, display_text, 0);
            let _ = write_sync(STDOUT, b"\n", 0);
        } else {
            // Non-navigable type (image, binary, etc.) — show but don't link
            let type_label = match item_type {
                b'g' | b'I' | b'p' => b"IMG",
                b'9' => b"BIN",
                b's' => b"SND",
                b'h' => b"HTM",
                _ => b"   ",
            };
            let _ = write_sync(STDOUT, b" ", 0);
            let _ = write_sync(STDOUT, type_label, 0);
            let _ = write_sync(STDOUT, b"      ", 0);
            let _ = write_sync(STDOUT, display_text, 0);
            let _ = write_sync(STDOUT, b"\n", 0);
        }
    }

    link_count
}

fn parse_port(s: &[u8]) -> Option<u16> {
    let mut result: u32 = 0;
    for &b in s {
        if b < b'0' || b > b'9' {
            return None;
        }
        result = result * 10 + (b - b'0') as u32;
        if result > 65535 {
            return None;
        }
    }
    if result == 0 { None } else { Some(result as u16) }
}

fn parse_usize(s: &[u8]) -> Option<usize> {
    let mut result: usize = 0;
    if s.is_empty() {
        return None;
    }
    for &b in s {
        if b < b'0' || b > b'9' {
            return None;
        }
        result = result * 10 + (b - b'0') as usize;
    }
    Some(result)
}

fn fmt_usize(buf: &mut [u8], mut val: usize) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 10];
    let mut len = 0;
    while val > 0 {
        tmp[len] = b'0' + (val % 10) as u8;
        val /= 10;
        len += 1;
    }
    for i in 0..len {
        buf[i] = tmp[len - 1 - i];
    }
    len
}
