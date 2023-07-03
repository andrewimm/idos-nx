//! Device driver for Intel e1000 ethernet controller, which is provded by
//! qemu and other emulators.

use alloc::vec::Vec;
use crate::collections::SlotList;
use crate::files::error::IOError;
use crate::filesystem::drivers::asyncfs::AsyncDriver;
use crate::filesystem::install_device_driver;
use crate::hardware::pci::devices::PciDevice;
use crate::hardware::pci::get_bus_devices;
use crate::interrupts::pic::install_interrupt_handler;
use crate::memory::address::PhysicalAddress;
use crate::net::register_network_interface;
use crate::task::actions::io::{open_pipe, transfer_handle, read_file, write_file};
use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::actions::memory::{map_memory, DmaRange};
use crate::task::actions::{yield_coop, read_message_blocking, send_message};
use crate::task::files::FileHandle;
use crate::task::memory::MemoryBacking;
use crate::task::switching::get_current_id;

use super::controller::E1000Controller;

/// Size of a single buffer used by a descriptor
const BUFFER_SIZE: usize = 1024;
/// Number of RX descriptors provided to the controller
const RX_DESC_COUNT: usize = 4;
/// Number of TX descriptors provided to the controller
const TX_DESC_COUNT: usize = 4;

pub struct EthernetDriver {
    controller: E1000Controller,

    rx_buffer_dma: DmaRange,
    tx_buffer_dma: DmaRange,
    descriptor_dma: DmaRange,

    rx_ring_index: usize,
    tx_ring_index: usize,

    open_handles: SlotList<()>,
}

impl EthernetDriver {
    pub fn new(controller: E1000Controller) -> Self {
        // Allocate 3 DMA buffers: raw RX bytes, raw TX bytes, and one
        // containing the descriptor rings
        let rx_buffer_dma = DmaRange::for_byte_length(BUFFER_SIZE * RX_DESC_COUNT).unwrap();
        let tx_buffer_dma = DmaRange::for_byte_length(BUFFER_SIZE * TX_DESC_COUNT).unwrap();

        // Store both the RX descriptor and TX descriptor rings in the same DMA
        // range, one after the other
        let rx_ring_length = core::mem::size_of::<RxDescriptor>() * RX_DESC_COUNT;
        let tx_ring_length = core::mem::size_of::<TxDescriptor>() * TX_DESC_COUNT;
        let descriptor_dma = DmaRange::for_byte_length(rx_ring_length + tx_ring_length).unwrap();

        let rd_ring_ptr = descriptor_dma.vaddr_start.as_ptr_mut::<RxDescriptor>();
        for i in 0..RX_DESC_COUNT {
            let desc = unsafe {
                &mut *rd_ring_ptr.add(i)
            };
            let offset = (i * BUFFER_SIZE) as u32;
            desc.addr_low = (rx_buffer_dma.paddr_start + offset).as_u32();
            desc.addr_high = 0;
        }

        let td_ring_offset = rx_ring_length as u32;
        let td_ring_ptr = (descriptor_dma.vaddr_start + td_ring_offset).as_ptr_mut::<TxDescriptor>();
        for i in 0..TX_DESC_COUNT {
            let desc = unsafe {
                &mut *td_ring_ptr.add(i)
            };
            let offset = (i * BUFFER_SIZE) as u32;
            desc.addr_low = (tx_buffer_dma.paddr_start + offset).as_u32();
            desc.addr_high = 0;
        }
        let td_ring_phys = descriptor_dma.paddr_start + td_ring_offset;

        // Set general configuration
        controller.set_flags(0, 1 << 26); // RST
        while controller.read_register(0) & (1 << 26) != 0 {
            // yield?
        }

        // Link reset, auto detect speed
        controller.set_flags(0, (1 << 5) | (1 << 6));
        
        // Set interrupt mask
        //controller.write_register(0xd0, 0xc0);
        controller.write_register(0xd0, 0);

        // Set the controller registers to point to all of the buffers that
        // have been allocated:

        // RDBAL - RX Descriptor Base Low
        controller.write_register(0x2800, descriptor_dma.paddr_start.as_u32());
        // RDBAH - RX Descriptor Base High
        controller.write_register(0x2804, 0);
        // RDLEN - RX Descriptor Length (in bytes)
        controller.write_register(0x2808, (RX_DESC_COUNT * core::mem::size_of::<RxDescriptor>()) as u32);
        // RDH - RX Descriptor Head
        controller.write_register(0x2810, 0);
        // RDT - RX Descriptor Tail
        controller.write_register(0x2818, RX_DESC_COUNT as u32 - 1);

        // RCTL: Enable; accept unicast, multicast; set packet size to 1024; strip CRC
        controller.clear_flags(0x100, 3 << 16);
        controller.set_flags(0x100, (1 << 1) | (1 << 3) | (1 << 15) | (1 << 16) | (1 << 26));

        // TDBAL - TX Descriptor Base Low
        controller.write_register(0x3800, td_ring_phys.as_u32());
        // TDBAH - TX Descriptor Base High
        controller.write_register(0x3804, 0);
        // TDLEN - TX Descriptor Length (in bytes)
        controller.write_register(0x3808, (TX_DESC_COUNT * core::mem::size_of::<TxDescriptor>()) as u32);
        // TDH - TX Descriptor Head
        controller.write_register(0x3810, 0);
        // TDT - TX Descriptor Tail
        controller.write_register(0x3818, 0);
        // TCTL: Enable, pad short packets
        controller.set_flags(0x400, (1 << 1) | (1 << 3)); 

        crate::kprint!("Wait for Link...\n");
        loop {
            if controller.read_register(0x08) & 2 == 2 {
                break;
            }
            yield_coop();
        }
        crate::kprint!("... Link established!\n");

        Self {
            controller,

            rx_buffer_dma,
            tx_buffer_dma,
            descriptor_dma,
            rx_ring_index: 0,
            tx_ring_index: 0,

            open_handles: SlotList::new(),
        }
    }

    fn get_rx_buffer(&self, index: usize) -> &mut [u8] {
        unsafe {
            let ptr = self.rx_buffer_dma
                .vaddr_start
                .as_ptr_mut::<u8>()
                .add(BUFFER_SIZE * index);
            core::slice::from_raw_parts_mut(ptr, BUFFER_SIZE)
        }
    }

    fn get_tx_buffer(&self, index: usize) -> &mut [u8] {
        unsafe {
            let ptr = self.tx_buffer_dma
                .vaddr_start
                .as_ptr_mut::<u8>()
                .add(BUFFER_SIZE * index);
            core::slice::from_raw_parts_mut(ptr, BUFFER_SIZE)
        }
    }

    fn get_tx_descriptor(&self, index: usize) -> &mut TxDescriptor {
        let rd_ring_length = (core::mem::size_of::<RxDescriptor>() * RX_DESC_COUNT) as u32;
        let td_ring_ptr = (self.descriptor_dma.vaddr_start + rd_ring_length).as_ptr_mut::<TxDescriptor>();
        unsafe {
            let ptr = td_ring_ptr.add(index);
            &mut *ptr
        }
    }

    fn next_tdesc_index(index: usize) -> usize {
        (index + 1) % TX_DESC_COUNT
    }

    pub fn tx(&mut self, data: &[u8]) -> usize {
        let cur_index = self.tx_ring_index;
        let tx_buffer = self.get_tx_buffer(cur_index);

        let write_length = tx_buffer.len().min(data.len());
        tx_buffer[0..write_length].copy_from_slice(&data[0..write_length]);

        let tx_descriptor = self.get_tx_descriptor(cur_index);
        tx_descriptor.length = write_length as u16;
        tx_descriptor.checksum_offset = 0;
        // Bit 3 - report status
        // Bit 1 - insert frame check sequence
        // Bit 0 - end of packet
        // We treat each send as a single packet, so we include the FCS and
        // mark the frame as ended
        tx_descriptor.command = 0b00001011;
        tx_descriptor.status = 0;
        tx_descriptor.checksum_start = 0;
        tx_descriptor.special = 0;

        let next_index = Self::next_tdesc_index(cur_index);
        self.tx_ring_index = next_index;
        // Update TX Descriptor Tail (TDT)
        self.controller.write_register(0x3818, next_index as u32);

        write_length
    }

    pub fn tx_struct<S: Sized>(&mut self, s: &S) -> usize {
        let buffer_ptr = s as *const S as *const u8;
        let buffer_size = core::mem::size_of::<S>();
        let buffer = unsafe {
            core::slice::from_raw_parts(buffer_ptr, buffer_size)
        };
        self.tx(buffer)
    }
}

impl AsyncDriver for EthernetDriver {
    fn open(&mut self, _path: &str) -> Result<u32, IOError> {
        Ok(self.open_handles.insert(()) as u32)
    }

    fn read(&mut self, _instance: u32, _buffer: &mut [u8]) -> Result<u32, IOError> {
        Ok(0)
    }

    fn write(&mut self, _instance: u32, buffer: &[u8]) -> Result<u32, IOError> {
        Ok(self.tx(buffer) as u32)
    }

    fn close(&mut self, handle: u32) -> Result<(), IOError> {
        if self.open_handles.remove(handle as usize).is_some() {
            Ok(())
        } else {
            Err(IOError::FileHandleInvalid)
        }
    }
}

#[repr(C, packed)]
pub struct RxDescriptor {
    addr_low: u32,
    addr_high: u32,
    length: u16,
    checksum: u16,
    status: u8,
    error: u8,
    special: u16,
}

#[repr(C, packed)]
pub struct TxDescriptor {
    addr_low: u32,
    addr_high: u32,
    length: u16,
    checksum_offset: u8,
    command: u8,
    status: u8,
    checksum_start: u8,
    special: u16,
}

static mut MAC_ADDR: [u8; 6] = [0; 6];

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

    if let Some(irq) = pci_dev.irq {
        install_interrupt_handler(irq as u32, interrupt_handler);
    }
    let mut driver_impl = EthernetDriver::new(controller);

    // Install as DEV:\\ETH
    let task_id = get_current_id();
    install_device_driver("ETH", task_id, 0).unwrap();

    crate::kprint!("Network driver installed as DEV:\\ETH\n");

    let _net_id = register_network_interface(mac);

    // inform the parent task
    write_file(response_writer, &[1]).unwrap(); 

    loop {
        let (message_read, _) = read_message_blocking(None);
        if let Some(packet) = message_read {
            let (sender, message) = packet.open();

            match driver_impl.handle_request(message) {
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
    //let controller = get_controller();
    //let interrupt_cause = controller.read_register(0xc0);
    //if interrupt_cause == 0 {
    //    return;
    //}


}

