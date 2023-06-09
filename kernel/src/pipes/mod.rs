pub mod fs;
pub mod pipe;

use alloc::boxed::Box;
use fs::PipeDriver;
pub use pipe::Pipe;
use spin::Once;

use crate::filesystem::{install_kernel_fs, drive::DriveID};

pub static PIPE_DRIVE_ID: Once<DriveID> = Once::new();

pub fn install_fs() {
    PIPE_DRIVE_ID.call_once(|| {
        install_kernel_fs("PIPE", Box::new(PipeDriver::new()))
    });
}

