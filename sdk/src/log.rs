extern crate alloc;

use alloc::collections::VecDeque;
use alloc::string::String;
use core::fmt;
use idos_api::io::sync::open_sync;
use idos_api::io::{write_op, AsyncOp, Handle};
use idos_api::syscall::io::{append_io_op, create_file_handle};

struct InFlightLog {
    _message: String,
    op: AsyncOp,
}

/// Asynchronous logger that writes to a LOG:\ endpoint.
///
/// Log messages are submitted as non-blocking async writes and kept alive
/// in a queue until the kernel signals completion. Each call to `log()`
/// drains completed entries from the front before submitting the new one.
pub struct SysLogger {
    handle: Handle,
    queue: VecDeque<InFlightLog>,
}

impl SysLogger {
    /// Create a new logger that writes to `LOG:\{tag}`.
    /// For example, `SysLogger::new("FATFS")` opens `LOG:\FATFS`.
    pub fn new(tag: &str) -> Self {
        let handle = create_file_handle();
        let mut path_buf = [0u8; 5 + 8]; // "LOG:\" + up to 8 char tag
        let prefix = b"LOG:\\";
        path_buf[..5].copy_from_slice(prefix);
        let tag_bytes = tag.as_bytes();
        let copy_len = tag_bytes.len().min(8);
        path_buf[5..5 + copy_len].copy_from_slice(&tag_bytes[..copy_len]);
        let path = unsafe { core::str::from_utf8_unchecked(&path_buf[..5 + copy_len]) };
        let _ = open_sync(handle, path, 0);

        Self {
            handle,
            queue: VecDeque::new(),
        }
    }

    /// Submit a log message asynchronously.
    pub fn log(&mut self, msg: &str) {
        self.drain_completed();

        let message = String::from(msg);
        let op = write_op(message.as_bytes(), 0);
        append_io_op(self.handle, &op, None);
        self.queue.push_back(InFlightLog {
            _message: message,
            op,
        });
    }

    /// Submit a formatted log message asynchronously.
    pub fn log_fmt(&mut self, args: fmt::Arguments) {
        use alloc::fmt::format;
        let message = format(args);
        self.drain_completed();

        let op = write_op(message.as_bytes(), 0);
        append_io_op(self.handle, &op, None);
        self.queue.push_back(InFlightLog {
            _message: message,
            op,
        });
    }

    fn drain_completed(&mut self) {
        while let Some(front) = self.queue.front() {
            if front.op.is_complete() {
                self.queue.pop_front();
            } else {
                break;
            }
        }
    }
}
