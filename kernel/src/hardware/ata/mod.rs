use crate::task::actions::handle::{
    create_kernel_task, create_pipe_handles, handle_op_read, handle_op_write_struct,
    transfer_handle,
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

        handle_op_write_struct(args_write, &message).wait_for_completion();

        handle_op_read(response_read, &mut [0u8], 0).wait_for_completion();
        driver_no += 1;
    }
}
