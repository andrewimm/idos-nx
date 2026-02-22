use crate::{
    collections::SlotList, files::path::Path, io::driver::kernel_driver::KernelDriver,
    log::TaggedLogger, random::get_random_bytes,
};
use idos_api::io::error::{IoError, IoResult};
use spin::RwLock;

use super::driver::AsyncIOCallback;

/// IOCTL code to set the ANSI color of a log channel.
/// Linux style, where the top 16 bits are a magic number (0x4C for 'L'ogger)
/// and the bottom 16 bits are a sequential command number (0x01 for set color).
/// Argument is the ANSI color code as a u32 (e.g. 34 = blue, 93 = bright yellow).
pub const LOG_IOCTL_SET_COLOR: u32 = 0x4C01;

/// Non-black ANSI colors that look good for log tags.
const LOG_COLORS: [u8; 12] = [
    31, 32, 33, 34, 35, 36, // red, green, yellow, blue, magenta, cyan
    91, 92, 93, 94, 95, 96, // bright variants
];

fn random_color() -> u8 {
    let mut byte = [0u8; 1];
    get_random_bytes(&mut byte);
    LOG_COLORS[(byte[0] as usize) % LOG_COLORS.len()]
}

struct LogChannel {
    logger: TaggedLogger,
}

/// A write-only filesystem that bridges userspace writes to the kernel's
/// TaggedLogger. Opening a file creates a named log channel; writing to it
/// emits tagged, colored messages to serial output.
///
/// Usage:
///   open("LOG:\FATFS")   → creates a channel tagged "FATFS"
///   write(handle, data)  → logs data as a tagged message
///   ioctl(SET_COLOR, 34) → changes the channel color to blue
pub struct LogFS {
    channels: RwLock<SlotList<LogChannel>>,
}

impl LogFS {
    pub fn new() -> Self {
        Self {
            channels: RwLock::new(SlotList::new()),
        }
    }
}

impl KernelDriver for LogFS {
    fn open(
        &self,
        path: Option<Path>,
        _flags: u32,
        _io_callback: AsyncIOCallback,
    ) -> Option<IoResult> {
        let path = match path {
            Some(p) if !p.is_empty() => p,
            _ => return Some(Err(IoError::InvalidArgument)),
        };

        let channel = LogChannel {
            logger: TaggedLogger::new(path.as_str(), random_color()),
        };
        let index = self.channels.write().insert(channel);
        Some(Ok(index as u32))
    }

    fn read(
        &self,
        _instance: u32,
        _buffer: &mut [u8],
        _offset: u32,
        _io_callback: AsyncIOCallback,
    ) -> Option<IoResult> {
        Some(Err(IoError::UnsupportedOperation))
    }

    fn write(
        &self,
        instance: u32,
        buffer: &[u8],
        _offset: u32,
        _io_callback: AsyncIOCallback,
    ) -> Option<IoResult> {
        let channels = self.channels.read();
        let channel = channels
            .get(instance as usize)
            .ok_or(IoError::FileHandleInvalid);
        let channel = match channel {
            Ok(c) => c,
            Err(e) => return Some(Err(e)),
        };

        let msg = core::str::from_utf8(buffer).unwrap_or("<invalid utf-8>");
        channel.logger.log(format_args!("{}", msg));

        Some(Ok(buffer.len() as u32))
    }

    fn ioctl(
        &self,
        instance: u32,
        ioctl: u32,
        arg: u32,
        _arg_len: usize,
        _io_callback: AsyncIOCallback,
    ) -> Option<IoResult> {
        match ioctl {
            LOG_IOCTL_SET_COLOR => {
                let mut channels = self.channels.write();
                let channel = channels
                    .get_mut(instance as usize)
                    .ok_or(IoError::FileHandleInvalid);
                let channel = match channel {
                    Ok(c) => c,
                    Err(e) => return Some(Err(e)),
                };
                // Rebuild the logger with the same tag but new color
                channel.logger = TaggedLogger::new(
                    core::str::from_utf8(&channel.logger.tag_bytes()).unwrap_or(""),
                    arg as u8,
                );
                Some(Ok(1))
            }
            _ => Some(Err(IoError::UnsupportedOperation)),
        }
    }

    fn close(&self, instance: u32, _io_callback: AsyncIOCallback) -> Option<IoResult> {
        if self.channels.write().remove(instance as usize).is_none() {
            Some(Err(IoError::FileHandleInvalid))
        } else {
            Some(Ok(1))
        }
    }
}
