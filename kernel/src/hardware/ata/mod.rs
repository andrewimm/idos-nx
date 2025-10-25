use alloc::vec::Vec;

use crate::hardware::pci::devices::PciDevice;
use crate::log::TaggedLogger;
use crate::task::actions::{
    handle::{create_kernel_task, create_pipe_handles, transfer_handle},
    io::{close_sync, read_sync, write_struct_sync, write_sync},
};

use super::pci::get_bus_devices;

pub mod controller;
pub mod driver;
pub mod protocol;

const LOGGER: TaggedLogger = TaggedLogger::new("ATA", 36);

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

    let mut installed_ata = 0;
    for bus_addr in supported {
        // each pci device has two separate ATA channels. Each channel needs to
        // be used by only one device at a time, and this can most easily be
        // done by creating a single task for each channel.
        let mut ports: [u16; 6] = [
            0x1F0, // Primary data port
            0x3F6, // Primary control port
            0,     // Primary bus master port
            0x170, // Secondary data port
            0x376, // Secondary control port
            0,     // Secondary bus master port
        ];
        let mut irqs: [u8; 2] = [14, 15];

        let pci_dev = PciDevice::read_from_bus(bus_addr[0], bus_addr[1], bus_addr[2]);
        let prog_if = pci_dev.programming_interface;
        if prog_if & 1 != 0 {
            // primary is in PCI native mode
            ports[0] = pci_dev.bar[0].unwrap().get_address() as u16;
            ports[1] = pci_dev.bar[1].unwrap().get_address() as u16 + 2;
            irqs[0] = pci_dev.irq.unwrap_or(14);
        }
        if prog_if & 4 != 0 {
            // secondary is in PCI native mode
            ports[3] = pci_dev.bar[2].unwrap().get_address() as u16;
            ports[4] = pci_dev.bar[3].unwrap().get_address() as u16 + 2;
            irqs[1] = pci_dev.irq.unwrap_or(15);
        }
        if prog_if & 0x80 != 0 {
            // bus mastering is enabled, use DMA
            ports[2] = pci_dev.bar[4].unwrap().get_address() as u16;
            ports[5] = pci_dev.bar[4].unwrap().get_address() as u16 + 8;
            pci_dev.enable_bus_master();
        }

        for i in 0..2 {
            let (args_read, args_write) = create_pipe_handles();
            let (response_read, response_write) = create_pipe_handles();
            let (_, task) = create_kernel_task(driver::run_driver, Some("ATADEV"));
            transfer_handle(args_read, task).unwrap();
            transfer_handle(response_write, task).unwrap();

            // send the args
            let driver_number: [u8; 1] = [installed_ata];
            let _ = write_sync(args_write, &driver_number, 0);
            let _ = write_struct_sync(
                args_write,
                &[ports[i * 3], ports[i * 3 + 1], ports[i * 3 + 2]],
            );
            let _ = write_sync(args_write, &irqs[i..(i + 1)], 0);
            // wait for response
            let mut response_buffer = [0u8; 1];
            let _ = read_sync(response_read, &mut response_buffer, 0);
            let _ = close_sync(args_write);
            let _ = close_sync(response_read);
            installed_ata += response_buffer[0];
        }
    }
}
