use crate::task::actions::{
    handle::{create_pipe_handles, handle_op_read, transfer_handle},
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
    handle_op_read(pipe_read, &mut drive_count, 0).wait_for_completion();
    crate::kprintln!("Floppy init: found {} drives", drive_count[0]);
}
