#![no_std]
#![no_main]

extern crate idos_api;
extern crate idos_sdk;

use core::sync::atomic::Ordering;

use idos_api::{
    io::{
        sync::{close_sync, open_sync, write_sync},
        AsyncOp, Handle, ASYNC_OP_READ,
    },
    syscall::{
        io::{append_io_op, block_on_wake_set, create_wake_set},
        net::create_tcp_handle,
    },
};

const STDOUT: Handle = Handle::new(1);
const MAX_CONNECTIONS: usize = 32;

struct Connection {
    handle: Handle,
    buf: [u8; 512],
    read_op: AsyncOp,
    number: u32,
}

#[no_mangle]
pub extern "C" fn main() {
    let port = parse_port();

    let listener = create_tcp_handle();
    if open_sync(listener, "0.0.0.0", port as u32).is_err() {
        let _ = write_sync(STDOUT, b"Failed to bind\n", 0);
        return;
    }

    let mut msg = [0u8; 64];
    let n = fmt_to_buf(&mut msg, format_args!("Listening on port {}\n", port));
    let _ = write_sync(STDOUT, &msg[..n], 0);

    let wake_set = create_wake_set();
    let listener_handle_val = listener.as_u32();

    // Connections table
    let mut connections: [Option<Connection>; MAX_CONNECTIONS] = Default::default();
    let mut connection_count: u32 = 0;

    // Issue first accept (read on listener)
    let mut accept_buf = [0u8; 4];
    let mut accept_op = AsyncOp::new(
        ASYNC_OP_READ,
        accept_buf.as_mut_ptr() as u32,
        accept_buf.len() as u32,
        0,
    );
    append_io_op(listener, &accept_op, Some(wake_set));

    loop {
        let woken = block_on_wake_set(wake_set, None);

        if woken == listener_handle_val {
            if !accept_op.is_complete() {
                continue;
            }

            let ret = accept_op.return_value.load(Ordering::SeqCst);
            if ret & 0x80000000 != 0 {
                // accept error, try again
                accept_op = AsyncOp::new(
                    ASYNC_OP_READ,
                    accept_buf.as_mut_ptr() as u32,
                    accept_buf.len() as u32,
                    0,
                );
                append_io_op(listener, &accept_op, Some(wake_set));
                continue;
            }

            let conn_handle = Handle::new(ret);
            connection_count += 1;

            // Find a free slot
            if let Some(slot) = connections.iter_mut().find(|s| s.is_none()) {
                let mut conn = Connection {
                    handle: conn_handle,
                    buf: [0u8; 512],
                    read_op: AsyncOp::new(ASYNC_OP_READ, 0, 0, 0), // placeholder
                    number: connection_count,
                };
                // Set up the read op pointing at the connection's buffer.
                // Must be done after conn is placed so the buffer address
                // is stable — but we're about to move conn into the slot,
                // so we set it up after the move.
                *slot = Some(conn);
                let conn = slot.as_mut().unwrap();
                conn.read_op = AsyncOp::new(
                    ASYNC_OP_READ,
                    conn.buf.as_mut_ptr() as u32,
                    conn.buf.len() as u32,
                    0,
                );
                append_io_op(conn_handle, &conn.read_op, Some(wake_set));
            } else {
                // No free slots, reject
                let _ = write_sync(conn_handle, b"HTTP/1.0 503 Service Unavailable\r\n\r\n", 0);
                let _ = close_sync(conn_handle);
            }

            // Re-issue accept
            accept_op = AsyncOp::new(
                ASYNC_OP_READ,
                accept_buf.as_mut_ptr() as u32,
                accept_buf.len() as u32,
                0,
            );
            append_io_op(listener, &accept_op, Some(wake_set));
        } else {
            // A connection handle woke — find it and respond
            let slot_idx = connections.iter().position(|s| {
                s.as_ref().map_or(false, |c| c.handle.as_u32() == woken)
            });

            if let Some(idx) = slot_idx {
                let conn = connections[idx].as_ref().unwrap();
                let conn_handle = conn.handle;
                let conn_number = conn.number;

                let mut resp = [0u8; 128];
                let len = fmt_to_buf(
                    &mut resp,
                    format_args!(
                        "HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\n\r\nConnection #{}\n",
                        conn_number,
                    ),
                );
                let _ = write_sync(conn_handle, &resp[..len], 0);
                let _ = close_sync(conn_handle);

                // Free the slot
                connections[idx] = None;
            }
        }
    }
}

fn parse_port() -> u16 {
    let mut args = idos_sdk::env::args();
    args.next(); // skip argv[0]
    match args.next() {
        Some(s) => {
            let mut result: u32 = 0;
            for b in s.as_bytes() {
                if *b < b'0' || *b > b'9' {
                    return 80;
                }
                result = result * 10 + (*b - b'0') as u32;
            }
            if result > 0 && result <= 65535 {
                result as u16
            } else {
                80
            }
        }
        None => 80,
    }
}

fn fmt_to_buf(buf: &mut [u8], args: core::fmt::Arguments) -> usize {
    let mut writer = BufWriter { buf, pos: 0 };
    let _ = core::fmt::write(&mut writer, args);
    writer.pos
}

struct BufWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> core::fmt::Write for BufWriter<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let remaining = self.buf.len() - self.pos;
        let to_write = bytes.len().min(remaining);
        self.buf[self.pos..self.pos + to_write].copy_from_slice(&bytes[..to_write]);
        self.pos += to_write;
        Ok(())
    }
}

// Required for Default on [Option<Connection>; MAX_CONNECTIONS]
impl Default for Connection {
    fn default() -> Self {
        Self {
            handle: Handle::new(0),
            buf: [0u8; 512],
            read_op: AsyncOp::new(0, 0, 0, 0),
            number: 0,
        }
    }
}
