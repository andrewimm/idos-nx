use crate::task::actions::{
    handle::{create_kernel_task, create_pipe_handles, transfer_handle},
    io::{read_sync, write_struct_sync},
};

pub mod controller;
pub mod driver;
pub mod protocol;

pub fn install() {
    let configs = [(0x1f0, 0x3f6), (0x170, 0x176)];

    let mut driver_no = 0;
    for (base_port, control_port) in configs {
        let (args_read, args_write) = create_pipe_handles();
        let (response_read, response_write) = create_pipe_handles();
        let (_, task) = create_kernel_task(driver::run_driver, Some("ATADEV"));
        transfer_handle(args_read, task).unwrap();
        transfer_handle(response_write, task).unwrap();
        let message: [u16; 3] = [driver_no, base_port, control_port];

        let _ = write_struct_sync(args_write, &message);

        let _ = read_sync(response_read, &mut [0u8], 0);
        driver_no += 1;
    }
}
