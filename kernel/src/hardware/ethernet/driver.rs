use crate::task::actions::memory::DmaRange;
use crate::task::actions::yield_coop;

use super::controller::E1000Controller;

/// Size of a single buffer used by a descriptor
pub const BUFFER_SIZE: usize = 1024;
/// Number of RX descriptors provided to the controller
pub const RX_DESC_COUNT: usize = 8;
/// Number of TX descriptors provided to the controller
pub const TX_DESC_COUNT: usize = 8;

// Consts for all the register numbers used

/// Device control register
const REG_CTRL: u16 = 0x00;
/// Interrupt cause
const REG_ICR: u16 = 0xc0;
/// Interrupt mask
const REG_IMS: u16 = 0xd0;
/// RX Control
const REG_RCTL: u16 = 0x100;
/// TX Control
const REG_TCTL: u16 = 0x400;
/// RX Descriptor base low
const REG_RDBAL: u16 = 0x2800;
/// RX Descriptor base high
const REG_RDBAH: u16 = 0x2804;
/// RX Descriptors total length, in bytes
const REG_RDLEN: u16 = 0x2808;
/// RX Descriptor ring head
const REG_RDH: u16 = 0x2810;
/// RX Descriptor ring tail
const REG_RDT: u16 = 0x2818;
/// TX Descriptor base low
const REG_TDBAL: u16 = 0x3800;
/// TX Descriptor base high
const REG_TDBAH: u16 = 0x3804;
/// TX Descriptors total length, in bytes
const REG_TDLEN: u16 = 0x3808;
/// TX Descriptor ring head
const REG_TDH: u16 = 0x3810;
/// TX Descriptor ring tail
const REG_TDT: u16 = 0x3818;

/// Driver implementation for the e1000 series of NIC
/// This handles the actual transmission and receipt of packets.
/// Note: The DMA buffers allocated for this driver are only accessible from
/// the task that created the driver instance.
pub struct EthernetDriver {
    controller: E1000Controller,

    // move this out into a shared object?
    rx_buffer_dma: DmaRange,
    tx_buffer_dma: DmaRange,
    descriptor_dma: DmaRange,

    rx_ring_index: usize,
    tx_ring_index: usize,
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
        controller.set_flags(REG_CTRL, 1 << 26); // RST
        while controller.read_register(REG_CTRL) & (1 << 26) != 0 {
            // yield?
        }

        // Link reset, auto detect speed
        controller.set_flags(REG_CTRL, (1 << 5) | (1 << 6));
        
        // Set interrupt mask
        controller.write_register(REG_IMS, 0xc0);

        // Set the controller registers to point to all of the buffers that
        // have been allocated:

        // RDBAL - RX Descriptor Base Low
        controller.write_register(REG_RDBAL, descriptor_dma.paddr_start.as_u32());
        // RDBAH - RX Descriptor Base High
        controller.write_register(REG_RDBAH, 0);
        // RDLEN - RX Descriptor Length (in bytes)
        controller.write_register(REG_RDLEN, (RX_DESC_COUNT * core::mem::size_of::<RxDescriptor>()) as u32);
        // RDH - RX Descriptor Head
        controller.write_register(REG_RDH, 0);
        // RDT - RX Descriptor Tail
        controller.write_register(REG_RDT, RX_DESC_COUNT as u32 - 1);

        // RCTL: Enable; accept unicast, multicast; set packet size to 1024; strip CRC
        controller.clear_flags(REG_RCTL, 3 << 16);
        controller.set_flags(REG_RCTL, (1 << 1) | (1 << 3) | (1 << 15) | (1 << 16) | (1 << 26));

        // TDBAL - TX Descriptor Base Low
        controller.write_register(REG_TDBAL, td_ring_phys.as_u32());
        // TDBAH - TX Descriptor Base High
        controller.write_register(REG_TDBAH, 0);
        // TDLEN - TX Descriptor Length (in bytes)
        let tdlen = TX_DESC_COUNT * core::mem::size_of::<TxDescriptor>();
        if tdlen & 127 != 0 {
            crate::kprintln!("TDLEN must be a multiple of 128. Invalid E1000 params");
        }
        controller.write_register(REG_TDLEN, tdlen as u32);
        // TDH - TX Descriptor Head
        controller.write_register(REG_TDH, 0);
        // TDT - TX Descriptor Tail
        controller.write_register(REG_TDT, 0);
        // TCTL: Enable, pad short packets
        controller.set_flags(REG_TCTL, (1 << 1) | (1 << 3)); 

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
        }
    }

    pub fn get_interrupt_cause(&self) -> u32 {
        self.controller.read_register(REG_ICR)
    }

    pub fn get_rx_buffer(&self, index: usize) -> &mut [u8] {
        unsafe {
            let ptr = self.rx_buffer_dma
                .vaddr_start
                .as_ptr_mut::<u8>()
                .add(BUFFER_SIZE * index);
            core::slice::from_raw_parts_mut(ptr, BUFFER_SIZE)
        }
    }

    pub fn get_tx_buffer(&self, index: usize) -> &mut [u8] {
        unsafe {
            let ptr = self.tx_buffer_dma
                .vaddr_start
                .as_ptr_mut::<u8>()
                .add(BUFFER_SIZE * index);
            core::slice::from_raw_parts_mut(ptr, BUFFER_SIZE)
        }
    }

    pub fn get_rx_descriptor(&self, index: usize) -> &mut RxDescriptor {
        let rd_ring_ptr = self.descriptor_dma.vaddr_start.as_ptr_mut::<RxDescriptor>();
        unsafe {
            let ptr = rd_ring_ptr.add(index);
            &mut *ptr
        }
    }

    pub fn get_tx_descriptor(&self, index: usize) -> &mut TxDescriptor {
        let rd_ring_length = (core::mem::size_of::<RxDescriptor>() * RX_DESC_COUNT) as u32;
        let td_ring_ptr = (self.descriptor_dma.vaddr_start + rd_ring_length).as_ptr_mut::<TxDescriptor>();
        unsafe {
            let ptr = td_ring_ptr.add(index);
            &mut *ptr
        }
    }

    pub fn get_next_rx_buffer(&self) -> Option<&mut[u8]> {
        let index = self.rx_ring_index;
        let desc = self.get_rx_descriptor(index);
        if !desc.is_done() {
            return None;
        }

        Some(self.get_rx_buffer(index))
    }

    pub fn mark_current_rx_read(&mut self) {
        let index = self.rx_ring_index;
        let desc = self.get_rx_descriptor(index);
        desc.clear_status();
        self.rx_ring_index = Self::next_rdesc_index(index);
        self.controller.write_register(REG_RDT, index as u32);
    }

    fn next_tdesc_index(index: usize) -> usize {
        (index + 1) % TX_DESC_COUNT
    }

    fn next_rdesc_index(index: usize) -> usize {
        (index + 1) % RX_DESC_COUNT
    }

    pub fn tx(&mut self, data: &[u8]) -> usize {
        let mut cur_index = self.tx_ring_index;

        let mut bytes_remaining = data.len();
        let mut bytes_written = 0;
        while bytes_remaining > 0 {
            let tx_buffer = self.get_tx_buffer(cur_index);
            let write_length = bytes_remaining.min(tx_buffer.len());
            tx_buffer[0..write_length].copy_from_slice(&data[bytes_written..(bytes_written + write_length)]);

            bytes_remaining -= write_length;
            bytes_written += write_length;

            let tx_desc = self.get_tx_descriptor(cur_index);
            tx_desc.length = write_length as u16;
            tx_desc.checksum_offset = 0;
            // bit 3 - report status
            // bit 1 - insert frame check sequence
            // bit 0 - end of packet
            // if we are splitting the packet contents across multiple
            // descriptors, we do not add FCS and EOP until the end
            tx_desc.command = if bytes_remaining == 0 {
                0b00001011
            } else {
                0b00001000
            };
            tx_desc.status = 0;
            tx_desc.checksum_start = 0;
            tx_desc.special = 0;

            cur_index = Self::next_tdesc_index(cur_index);
        }
        self.tx_ring_index = cur_index;
        self.controller.write_register(REG_TDT, cur_index as u32);
        bytes_written
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

#[derive(Debug, Copy, Clone)]
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

impl RxDescriptor {
    pub fn is_packet_end(&self) -> bool {
        self.status & 2 != 0
    }

    pub fn is_done(&self) -> bool {
        self.status & 1 != 0
    }

    pub fn clear_status(&mut self) {
        self.status = 0;
    }

    pub fn get_length(&self) -> usize {
        self.length as usize
    }
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


