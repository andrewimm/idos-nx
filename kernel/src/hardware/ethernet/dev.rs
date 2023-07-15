//! Device driver for Intel e1000 ethernet controller, which is provded by
//! qemu and other emulators.

use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering};

use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::collections::SlotList;
use crate::files::error::IOError;
use crate::filesystem::drivers::asyncfs::AsyncDriver;
use crate::filesystem::install_device_driver;
use crate::hardware::pci::devices::PciDevice;
use crate::hardware::pci::get_bus_devices;
use crate::interrupts::pic::install_interrupt_handler;
use crate::memory::address::PhysicalAddress;
use crate::net::{register_network_interface, notify_net_device_ready};
use crate::task::actions::io::{open_pipe, transfer_handle, read_file, write_file};
use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::actions::memory::map_memory;
use crate::task::actions::{read_message_blocking, send_message};
use crate::task::files::FileHandle;
use crate::task::memory::MemoryBacking;
use crate::task::switching::get_current_id;

use super::controller::E1000Controller;
use super::driver::EthernetDriver;

pub struct EthernetDevice {
    driver: Arc<RefCell<EthernetDriver>>,
    open_handles: SlotList<()>,
}

impl EthernetDevice {
    pub fn new(driver: Arc<RefCell<EthernetDriver>>) -> Self {
        Self {
            driver,
            open_handles: SlotList::new(),
        }
    }
}

impl AsyncDriver for EthernetDevice {
    fn open(&mut self, _path: &str) -> Result<u32, IOError> {
        Ok(self.open_handles.insert(()) as u32)
    }

    fn read(&mut self, _instance: u32, buffer: &mut [u8]) -> Result<u32, IOError> {
        let mut driver = self.driver.borrow_mut();

        let read_len = match driver.get_next_rx_buffer() {
            Some(rx_buffer) => {
                let read_len = rx_buffer.len().min(buffer.len());
                buffer[..read_len].copy_from_slice(&rx_buffer[..read_len]);
                read_len
            },
            None => return Ok(0),
        };

        driver.mark_current_rx_read();

        Ok(read_len as u32)
    }

    fn write(&mut self, _instance: u32, buffer: &[u8]) -> Result<u32, IOError> {
        Ok(self.driver.borrow_mut().tx(buffer) as u32)
    }

    fn close(&mut self, handle: u32) -> Result<(), IOError> {
        if self.open_handles.remove(handle as usize).is_some() {
            Ok(())
        } else {
            Err(IOError::FileHandleInvalid)
        }
    }
}

static mut MAC_ADDR: [u8; 6] = [0; 6];

static mut DRIVER: Option<Arc<RefCell<EthernetDriver>>> = None;

static NET_DEV_ID: AtomicU32 = AtomicU32::new(0);

fn get_driver() -> Arc<RefCell<EthernetDriver>> {
    unsafe {
        DRIVER.clone().unwrap()
    }
}

fn run_driver() -> ! {
    let args_reader = FileHandle::new(0);
    let response_writer = FileHandle::new(1);

    let mut args: [u8; 3] = [0; 3];
    read_file(args_reader, &mut args).unwrap();

    crate::kprint!("Install Ethernet driver for PCI device at {:x}:{:x}:{:x}\n", args[0], args[1], args[2]);
    let pci_dev = PciDevice::read_from_bus(args[0], args[1], args[2]);
    // bus mastering is needed to perform DMA
    pci_dev.enable_bus_master();
    let mmio_location = pci_dev.bar[0].unwrap().get_address();
    let mmio_address = map_memory(
        None,
        0x10000,
        MemoryBacking::Direct(PhysicalAddress::new(mmio_location)),
    ).unwrap();

    let controller = E1000Controller::with_mmio(mmio_address);

    let mac = controller.get_mac_address();
    crate::kprint!(
        "Ethernet MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}\n",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5],
    );

    unsafe {
        MAC_ADDR = mac;
    }

    let task_id = get_current_id();

    if let Some(irq) = pci_dev.irq {
        install_interrupt_handler(irq as u32, interrupt_handler, Some(task_id));
    }
    let driver = Arc::new(RefCell::new(EthernetDriver::new(controller)));
    unsafe {
        DRIVER = Some(driver.clone());
    }

    let mut device_impl = EthernetDevice::new(driver);

    // Install as DEV:\\ETH
    install_device_driver("ETH", task_id, 0).unwrap();

    crate::kprint!("Network driver installed as DEV:\\ETH\n");

    let net_id = register_network_interface(mac, "DEV:\\ETH");
    NET_DEV_ID.store(*net_id, Ordering::SeqCst);

    // inform the parent task
    write_file(response_writer, &[1]).unwrap(); 

    loop {
        let (message_read, _) = read_message_blocking(None);
        if let Some(packet) = message_read {
            let (sender, message) = packet.open();

            match device_impl.handle_request(message) {
                Some(response) => send_message(sender, response, 0xffffffff),
                None => continue,
            }
        }
    }
}

pub fn install_driver() {
    let pci_devices = get_bus_devices();
    let supported: Vec<[u8; 3]> = pci_devices.into_iter()
        .filter(|dev| {
            dev.vendor_id == 0x8086 &&
            dev.device_id == 0x100e
        })
        .map(|dev| [dev.bus, dev.device, dev.function])
        .collect();

    if supported.is_empty() {
        return;
    }

    let bus_addr = supported.get(0).unwrap();

    let (args_reader, args_writer) = open_pipe().unwrap();
    let (response_reader, response_writer) = open_pipe().unwrap();

    let driver_task = create_kernel_task(run_driver, Some("ETHDEV"));
    transfer_handle(args_reader, driver_task).unwrap();
    transfer_handle(response_writer, driver_task).unwrap();
    
    // send the PCI identifier to the driver
    write_file(args_writer, bus_addr).unwrap();

    // wait for a response from the driver indicating initialization
    read_file(response_reader, &mut [0u8]).unwrap();
}

pub fn interrupt_handler(_irq: u32) {
    crate::kprintln!("NET IRQ");
    let driver = get_driver();
    let interrupt_cause = driver.borrow().get_interrupt_cause();
    if interrupt_cause == 0 {
        return;
    }

    crate::kprintln!("Int cause: {:X}", interrupt_cause);

    let net_dev_id = NET_DEV_ID.load(Ordering::SeqCst);
    
    notify_net_device_ready(net_dev_id);

    for i in 0..super::driver::RX_DESC_COUNT {
        let desc = driver.borrow().get_rx_descriptor(i).clone();
        crate::kprintln!("{:?}", desc);
    }
}

