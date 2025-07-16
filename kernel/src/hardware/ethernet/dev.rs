//! Device driver for Intel e1000 ethernet controller, which is provided by
//! qemu and other emulators.
//!
//! This driver runs an event loop, waiting for incoming IO messages or hardware
//! interrupts. When a read request comes in, it checks the status of the
//! hardware to see if data is already available. If not, it stores information
//! necessary to complete the async request and waits for a hardware interrupt.
//! On every interrupt, if the cause was a received packet, it checks if there
//! is a pending read request and completes it.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::hardware::pci::devices::PciDevice;
use crate::hardware::pci::get_bus_devices;
use crate::io::driver::comms::{DriverCommand, IOResult};
use crate::io::filesystem::install_task_dev;
use crate::io::handle::Handle;
use crate::io::IOError;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::shared::release_buffer;
use crate::net::resident::register_network_device;
use crate::task::actions::handle::{
    create_pipe_handles, open_interrupt_handle, open_message_queue, transfer_handle,
};
use crate::task::actions::io::{close_sync, driver_io_complete, read_sync, send_io_op, write_sync};
use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::actions::memory::map_memory;
use crate::task::actions::sync::{block_on_wake_set, create_wake_set};
use crate::task::memory::MemoryBacking;
use crate::task::messaging::Message;
use alloc::vec::Vec;
use idos_api::io::{AsyncOp, ASYNC_OP_READ};

use super::controller::E1000Controller;
use super::driver::EthernetDriver;

pub struct EthernetDevice {
    driver: EthernetDriver,
    next_instance: AtomicU32,
    pending_read: Option<(*mut u8, usize, u32)>,
}

impl EthernetDevice {
    pub fn new(driver: EthernetDriver) -> Self {
        Self {
            driver,
            next_instance: AtomicU32::new(1),
            pending_read: None,
        }
    }

    // TODO: we need some async variant of the driver trait

    pub fn handle_request(&mut self, message: Message) -> Option<IOResult> {
        match DriverCommand::from_u32(message.message_type) {
            DriverCommand::OpenRaw => Some(self.open()),
            DriverCommand::Close => Some(self.close()),
            DriverCommand::Read => {
                let buffer_ptr = message.args[1] as *mut u8;
                let buffer_len = message.args[2] as usize;
                if let Some(response) = self.read(buffer_ptr, buffer_len) {
                    return Some(response);
                }
                // Response is not ready yet, store the buffer info for when the
                // task wakes from an interrupt
                self.pending_read
                    .replace((buffer_ptr, buffer_len, message.unique_id));

                None
            }
            DriverCommand::Write => {
                let buffer_ptr = message.args[1] as *const u8;
                let buffer_len = message.args[2] as usize;
                Some(self.write(buffer_ptr, buffer_len))
            }
            _ => Some(Err(IOError::UnsupportedOperation)),
        }
    }

    pub fn open(&mut self) -> IOResult {
        let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
        Ok(instance)
    }

    pub fn close(&mut self) -> IOResult {
        return Ok(1);
    }

    pub fn read(&mut self, buffer_ptr: *mut u8, buffer_len: usize) -> Option<IOResult> {
        let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
        let rx_buffer = self.driver.get_next_rx_buffer()?;
        let read_len = rx_buffer.len().min(buffer.len());
        buffer[..read_len].copy_from_slice(&rx_buffer[..read_len]);
        self.driver.mark_current_rx_read();
        release_buffer(VirtualAddress::new(buffer_ptr as u32), buffer_len);
        Some(Ok(read_len as u32))
    }

    pub fn write(&mut self, buffer_ptr: *const u8, buffer_len: usize) -> IOResult {
        let buffer = unsafe { core::slice::from_raw_parts(buffer_ptr, buffer_len) };
        Ok(self.driver.tx(buffer) as u32)
    }
}

fn run_driver() -> ! {
    let args_reader = Handle::new(0);
    let response_writer = Handle::new(1);

    let mut args: [u8; 3] = [0; 3];
    let _ = read_sync(args_reader, &mut args, 0);
    crate::kprintln!(
        "Install Ethernet driver for PCI device at {:x}:{:x}:{:x}",
        args[0],
        args[1],
        args[2]
    );
    let pci_dev = PciDevice::read_from_bus(args[0], args[1], args[2]);
    // bus mastering is needed to perform DMA
    pci_dev.enable_bus_master();
    let mmio_location = pci_dev.bar[0].unwrap().get_address();
    let mmio_address = map_memory(
        None,
        0x10000,
        MemoryBacking::Direct(PhysicalAddress::new(mmio_location)),
    )
    .unwrap();

    let controller = E1000Controller::with_mmio(mmio_address);
    let mac = controller.get_mac_address();
    crate::kprintln!(
        "Ethernet MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5],
    );

    let eth = EthernetDriver::new(controller);
    let mut driver_impl = EthernetDevice::new(eth);

    let interrupt_handle = if let Some(irq) = pci_dev.irq {
        open_interrupt_handle(irq)
    } else {
        panic!("No PCI IRQ");
    };
    let messages_handle = open_message_queue();
    let wake_set = create_wake_set();

    let mut incoming_message = Message::empty();
    let mut interrupt_ready: [u8; 1] = [0; 1];

    let mut message_read = AsyncOp::new(
        ASYNC_OP_READ,
        &mut incoming_message as *mut Message as u32,
        core::mem::size_of::<Message>() as u32,
        0,
    );
    let _ = send_io_op(messages_handle, &message_read, Some(wake_set));
    let mut interrupt_read = AsyncOp::new(ASYNC_OP_READ, interrupt_ready.as_mut_ptr() as u32, 1, 0);
    let _ = send_io_op(interrupt_handle, &interrupt_read, Some(wake_set));

    register_network_device("DEV:\\ETH", mac);

    let _ = write_sync(response_writer, &[0], 0);
    let _ = close_sync(response_writer);

    loop {
        if interrupt_read.is_complete() {
            let cause = driver_impl.driver.get_interrupt_cause();
            if cause != 0 {
                // check if a buffer can be read
                if driver_impl.driver.get_next_rx_buffer().is_some() {
                    if let Some((buffer_ptr, buffer_len, unique_id)) =
                        driver_impl.pending_read.take()
                    {
                        if let Some(response) = driver_impl.read(buffer_ptr, buffer_len) {
                            send_response(unique_id, response);
                        } else {
                            driver_impl.pending_read = Some((buffer_ptr, buffer_len, unique_id));
                        }
                    }
                }
            }

            let _ = write_sync(interrupt_handle, &[], 0);

            interrupt_read = AsyncOp::new(ASYNC_OP_READ, interrupt_ready.as_mut_ptr() as u32, 1, 0);
            let _ = send_io_op(interrupt_handle, &interrupt_read, Some(wake_set));
        } else if message_read.is_complete() {
            match driver_impl.handle_request(incoming_message) {
                Some(response) => send_response(incoming_message.unique_id, response),
                None => (),
            }

            message_read = AsyncOp::new(
                ASYNC_OP_READ,
                &mut incoming_message as *mut Message as u32,
                core::mem::size_of::<Message>() as u32,
                0,
            );
            let _ = send_io_op(messages_handle, &message_read, Some(wake_set));
        } else {
            block_on_wake_set(wake_set, None);
        }
    }
}

fn send_response(request_id: u32, result: IOResult) {
    driver_io_complete(request_id, result);
}

pub fn install_driver() {
    let pci_devices = get_bus_devices();
    let supported: Vec<[u8; 3]> = pci_devices
        .into_iter()
        .filter(|dev| dev.vendor_id == 0x8086 && dev.device_id == 0x100e)
        .map(|dev| [dev.bus, dev.device, dev.function])
        .collect();

    if supported.is_empty() {
        return;
    }

    let bus_addr = supported.get(0).unwrap();

    let (args_reader, args_writer) = create_pipe_handles();
    let (response_reader, response_writer) = create_pipe_handles();

    let driver_task = create_kernel_task(run_driver, Some("ETHDEV"));
    transfer_handle(args_reader, driver_task);
    transfer_handle(response_writer, driver_task);

    // send the PCI identifier to the driver
    let _ = write_sync(args_writer, bus_addr, 0);
    // wait for a response from the driver indicating initialization
    let _ = read_sync(response_reader, &mut [0u8], 0);

    install_task_dev("ETH", driver_task, 0);
}
