pub mod driver;
pub mod fs;
pub mod pipe;

use alloc::boxed::Box;
use fs::PipeDriver;
pub use fs::create_pipe;
pub use pipe::Pipe;
use spin::Once;

use crate::filesystem::{install_kernel_fs, drive::DriveID};

pub static PIPE_DRIVE_ID: Once<DriveID> = Once::new();

pub fn install_fs() {
    PIPE_DRIVE_ID.call_once(|| {
        install_kernel_fs("PIPE", Box::new(PipeDriver::new()))
    });
}

pub fn get_pipe_drive_id() -> DriveID {
    *PIPE_DRIVE_ID.get().expect("PIPE FS not initialized")
}

#[cfg(test)]
mod tests {
    #[test_case]
    fn pipe_fs() {
        let (read_handle, write_handle) = crate::task::actions::io::open_pipe().unwrap();

        let written = crate::task::actions::io::write_file(write_handle, "ABCDE".as_bytes()).unwrap();
        assert_eq!(written, 5);
        
        let mut buffer: [u8; 4] = [0; 4];
        let read = crate::task::actions::io::read_file(read_handle, &mut buffer).unwrap();
        crate::kprint!("DONE\n");
        assert_eq!(read, 4);
        assert_eq!(buffer, [b'A', b'B', b'C', b'D']);
    }
}

