use alloc::vec::Vec;

use crate::task::actions::{
    handle::{create_kernel_task, create_pipe_handles, transfer_handle},
    io::{close_sync, read_sync, write_struct_sync, write_sync},
};

use super::pci::get_bus_devices;

pub mod controller;
pub mod driver;
//pub mod pci;
pub mod protocol;

pub fn install() {
    let pci_devices = get_bus_devices();
    let supported: Vec<[u8; 3]> = pci_devices
        .into_iter()
        .filter(|dev| dev.class_code == 1 && dev.subclass == 1)
        .map(|dev| [dev.bus, dev.device, dev.function])
        .collect();

    if supported.is_empty() {
        return;
    }

    let mut driver_no = 0;
    for bus_addr in supported {
        let (args_read, args_write) = create_pipe_handles();
        let (response_read, response_write) = create_pipe_handles();
        let (_, task) = create_kernel_task(driver::run_driver, Some("ATADEV"));
        transfer_handle(args_read, task).unwrap();
        transfer_handle(response_write, task).unwrap();
        let message: [u8; 4] = [driver_no, bus_addr[0], bus_addr[1], bus_addr[2]];

        let _ = write_sync(args_write, &message, 0);

        let _ = read_sync(response_read, &mut [0u8], 0);
        driver_no += 1;
    }
}
