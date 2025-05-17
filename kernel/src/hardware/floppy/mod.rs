use crate::task::actions::{
    handle::{create_pipe_handles, transfer_handle},
    io::read_sync,
    lifecycle::create_kernel_task,
};

pub mod controller;
pub mod driver;
pub mod geometry;

pub fn install() {
    let (pipe_read, pipe_write) = create_pipe_handles();
    let driver_task = create_kernel_task(driver::run_driver, Some("FDDEV"));
    transfer_handle(pipe_write, driver_task);

    let mut drive_count: [u8; 1] = [0];
    let _ = read_sync(pipe_read, &mut drive_count, 0);
    crate::kprintln!("Floppy init: found {} drives", drive_count[0]);
}
