//! Device driver for Intel e1000 ethernet controller, which is provded by
//! qemu and other emulators.

use crate::hardware::ethernet::frame::EthernetFrame;
use crate::memory::address::PhysicalAddress;
use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::actions::memory::map_memory;
use crate::task::actions::yield_coop;
use crate::task::memory::MemoryBacking;
use crate::task::paging::page_on_demand;

use super::controller::E1000Controller;

/// Size of a single buffer used by a descriptor
const BUFFER_SIZE: usize = 1024;
/// Number of RX descriptors provided to the controller
const RX_DESC_COUNT: usize = 4;
/// Number of TX descriptors provided to the controller
const TX_DESC_COUNT: usize = 4;

pub struct EthernetDriver {
    controller: E1000Controller,

    rx_buffer_ptr: *mut u8,
    rx_buffer_len: usize,

    tx_buffer_ptr: *mut u8,
    tx_buffer_len: usize,

    rx_ring_ptr: *mut RxDescriptor,
    tx_ring_ptr: *mut TxDescriptor,
    tx_ring_index: usize,
}

impl EthernetDriver {
    pub fn new(controller: E1000Controller) -> Self {
        // Allocate 3 DMA buffers: raw RX bytes, raw TX bytes, and one
        // containing the descriptor rings
        let mut rx_buffer_space = BUFFER_SIZE * RX_DESC_COUNT;
        // round up to the nearest page
        if rx_buffer_space & 0xfff != 0 {
            rx_buffer_space &= 0xfffff000;
            rx_buffer_space += 0x1000;
        }
        let mut tx_buffer_space = BUFFER_SIZE * TX_DESC_COUNT;
        if tx_buffer_space & 0xfff != 0 {
            tx_buffer_space &= 0xfffff000;
            tx_buffer_space += 0x1000;
        }

        let rx_buffer_address = map_memory(None, rx_buffer_space as u32, MemoryBacking::DMA).unwrap();
        let rx_buffer_phys = page_on_demand(rx_buffer_address).unwrap();
        let tx_buffer_address = map_memory(None, tx_buffer_space as u32, MemoryBacking::DMA).unwrap();
        let tx_buffer_phys = page_on_demand(tx_buffer_address).unwrap();

        let rx_ring_space = core::mem::size_of::<RxDescriptor>() * RX_DESC_COUNT;
        let tx_ring_space = core::mem::size_of::<TxDescriptor>() * TX_DESC_COUNT;

        let mut ring_space = rx_ring_space + tx_ring_space;
        if ring_space & 0xfff != 0 {
            ring_space &= 0xfffff000;
            ring_space += 0x1000;
        }

        let descriptor_buffer_address = map_memory(None, ring_space as u32, MemoryBacking::DMA).unwrap();
        let descriptor_buffer_phys = page_on_demand(descriptor_buffer_address).unwrap();

        let rx_ring_ptr = descriptor_buffer_address.as_ptr_mut::<RxDescriptor>();
        for i in 0..RX_DESC_COUNT {
            let desc = unsafe {
                &mut *rx_ring_ptr.add(i)
            };
            let offset = (i * BUFFER_SIZE) as u32;
            desc.addr_low = (rx_buffer_phys + offset).as_u32();
            desc.addr_high = 0;
        }
        let tx_ring_offset = rx_ring_space as u32;
        let tx_ring_ptr = (descriptor_buffer_address + tx_ring_offset).as_ptr_mut::<TxDescriptor>();
        let tx_ring_phys = descriptor_buffer_phys + tx_ring_offset;
        for i in 0..TX_DESC_COUNT {
            let desc = unsafe {
                &mut *tx_ring_ptr.add(i)
            };
            let offset = (i * BUFFER_SIZE) as u32;
            desc.addr_low = (tx_buffer_phys + offset).as_u32();
            desc.addr_high = 0;
        }

        // Set general configuration
        controller.set_flags(0, 1 << 26); // RST
        while controller.read_register(0) & (1 << 26) != 0 {
            // yield?
        }

        // Link reset, auto detect speed
        controller.set_flags(0, (1 << 5) | (1 << 6));
        
        // Set interrupt mask
        controller.write_register(0xd0, 0);

        // Set the controller registers to point to all of the buffers that
        // have been allocated:

        // RDBAL - RX Descriptor Base Low
        controller.write_register(0x2800, descriptor_buffer_phys.as_u32());
        // RDBAH - RX Descriptor Base High
        controller.write_register(0x2804, 0);

        // TDBAL - TX Descriptor Base Low
        controller.write_register(0x3800, tx_ring_phys.as_u32());
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

            rx_buffer_ptr: rx_buffer_address.as_ptr_mut::<u8>(),
            rx_buffer_len: rx_buffer_space,

            tx_buffer_ptr: tx_buffer_address.as_ptr_mut::<u8>(),
            tx_buffer_len: tx_buffer_space,

            rx_ring_ptr,
            tx_ring_ptr,

            tx_ring_index: 0,
        }
    }

    fn get_rx_buffer(&self, index: usize) -> &mut [u8] {
        unsafe {
            let ptr = self.rx_buffer_ptr.add(BUFFER_SIZE * index);
            core::slice::from_raw_parts_mut(ptr, BUFFER_SIZE)
        }
    }

    fn get_tx_buffer(&self, index: usize) -> &mut [u8] {
        unsafe {
            let ptr = self.tx_buffer_ptr.add(BUFFER_SIZE * index);
            core::slice::from_raw_parts_mut(ptr, BUFFER_SIZE)
        }
    }

    fn get_tx_descriptor(&self, index: usize) -> &mut TxDescriptor {
        unsafe {
            let ptr = self.tx_ring_ptr.add(index);
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
        //self.tx_ring_index = next_index;
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

#[repr(C, packed)]
pub struct ARP {
    hardware_type: u16,
    protocol_type: u16,
    hardware_addr_length: u8,
    protocol_addr_length: u8,
    opcode: u16,
    source_hardware_addr: [u8; 6],
    source_protocol_addr: [u8; 4],
    dest_hardware_addr: [u8; 6],
    dest_protocol_addr: [u8; 4],
}

impl ARP {
    pub fn request(src_mac: [u8; 6], src_ip: [u8; 4], lookup: [u8; 4]) -> Self {
        Self {
            hardware_type: 1u16.to_be(),
            protocol_type: 0x0800u16.to_be(),
            hardware_addr_length: 6,
            protocol_addr_length: 4,
            opcode: 1u16.to_be(),
            source_hardware_addr: src_mac,
            source_protocol_addr: src_ip,
            dest_hardware_addr: [0; 6],
            dest_protocol_addr: lookup,
        }
    }

    pub fn response(src_mac: [u8; 6], src_ip: [u8; 4], dest_mac: [u8; 6], dest_ip: [u8; 4]) -> Self {
        Self {
            hardware_type: 1u16.to_be(),
            protocol_type: 0x0800u16.to_be(),
            hardware_addr_length: 6,
            protocol_addr_length: 4,
            opcode: 2u16.to_be(),
            source_hardware_addr: src_mac,
            source_protocol_addr: src_ip,
            dest_hardware_addr: dest_mac,
            dest_protocol_addr: dest_ip,
        }
    }

    /// Respond to an ARP request packet with the system MAC and IP
    pub fn respond(&self, mac: [u8; 6], ip: [u8; 4]) -> Option<Self> {
        if self.opcode != 1u16.to_be() {
            return None;
        }
        let response = Self::response(mac, ip, self.source_hardware_addr, self.source_protocol_addr);
        Some(response)
    }

    pub fn announce(mac: [u8; 6], ip: [u8; 4]) -> Self {
        Self::request(mac, ip, ip)
    }
}

fn run_driver() -> ! {
    let mmio_address = map_memory(
        None,
        0x10000,
        MemoryBacking::Direct(PhysicalAddress::new(0xfebc0000)),
    ).unwrap();

    let controller = E1000Controller::new(mmio_address);

    let mac = controller.get_mac_address();
    crate::kprint!(
        "Ethernet MAC: {:X}:{:X}:{:X}:{:X}:{:X}:{:X}\n",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5],
    );

    // enable bus mastering
    // this is cheating, it needs to actually read the pci bus
    let pci_config = crate::hardware::pci::config::read_config_u32(0, 3, 0, 4);
    crate::hardware::pci::config::write_config_u32(0, 3, 0, 4, pci_config | 4);

    let mut driver_impl = EthernetDriver::new(controller);

    let send_buffer_size = core::mem::size_of::<EthernetFrame>() + core::mem::size_of::<ARP>();
    let mut send_buffer_vec = alloc::vec![0u8; send_buffer_size];
    let send_buffer = &mut send_buffer_vec[0..send_buffer_size];
    unsafe {
        let frame = &mut *(&mut send_buffer[0] as *mut u8 as *mut EthernetFrame);
        *frame = EthernetFrame::broadcast_arp(mac);
        let arp_offset = core::mem::size_of::<EthernetFrame>();
        let arp = &mut *(&mut send_buffer[arp_offset] as *mut u8 as *mut ARP);
        *arp = ARP::announce(mac, [192, 168, 20, 12]);

        driver_impl.tx(send_buffer);
    }

    loop {
        crate::task::actions::lifecycle::wait_for_io(None);
        yield_coop();
    }
}

pub fn install_driver() {
    // TODO: actually crawl the device tree and look for supported PCI devices
    // Then, use the BAR registers to find the appropriate IO port numbers, etc

    let task = create_kernel_task(run_driver);
}

pub fn interrupt_handler(_irq: u32) {
    
}

