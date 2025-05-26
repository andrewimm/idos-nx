use alloc::string::String;

use super::protocol::{extract_ata_string, AtaCommand};
use crate::arch::port::Port;
use crate::io::handle::Handle;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::task::actions::io::{read_sync, write_sync};
use crate::task::actions::memory::{map_memory, unmap_memory_for_task};
use crate::task::actions::{sleep, yield_coop};
use crate::task::memory::MemoryBacking;
use crate::task::paging::get_current_physical_address;
use crate::task::switching::get_current_id;

pub const SECTOR_SIZE: usize = 512;

/// Each ATA Channel has up to two connected drives. The channel needs to be
/// told which disk to use before each command.
#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum DriveSelect {
    Primary = 0x00,
    Secondary = 0x10,
}

/// PRD Table for DMA transfers
/// On creation, it allocates a buffer for the PRD table. A DMA buffer can be
/// added to the PRDT, which will allocate the correct entries in the table.
/// When the PRDT struct is dropped, it will free the allocated buffer.
struct PRDT {
    table_vaddr: VirtualAddress,
    table_paddr: PhysicalAddress,
}

#[repr(C, packed)]
struct PRDEntry {
    paddr: PhysicalAddress,
    size: u16,
    end_of_table: u16,
}

impl PRDT {
    pub fn new() -> Self {
        let table_vaddr = map_memory(None, 0x1000, MemoryBacking::Anonymous).unwrap();
        unsafe {
            // force a page fault to fill the page
            core::ptr::write_volatile(table_vaddr.as_ptr_mut::<u8>(), 0);
        }
        let table_paddr = get_current_physical_address(table_vaddr).unwrap();

        Self {
            table_vaddr,
            table_paddr,
        }
    }

    pub fn entries(&self) -> &'static mut [PRDEntry] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.table_vaddr.as_ptr_mut::<PRDEntry>(),
                0x1000 / core::mem::size_of::<PRDEntry>(),
            )
        }
    }

    pub fn set_buffer(&self, buffer_paddr: PhysicalAddress, size: u32) {
        let entries = self.entries();

        let mut current_entry = 0;
        let mut current_paddr = buffer_paddr;

        let mut size_remaining = size;
        while size_remaining > 0 {
            let mut chunk_size = size_remaining;
            let offset_64k = current_paddr.as_u32() & 0xffff;
            let bytes_to_next_64k = 0x10000 - offset_64k;
            // only DMA up to the next 64K boundary
            if chunk_size > bytes_to_next_64k {
                chunk_size = bytes_to_next_64k;
            }
            if chunk_size > 0x10000 {
                chunk_size = 0x10000;
            }

            if chunk_size == 0x10000 {
                entries[current_entry].size = 0;
            } else {
                entries[current_entry].size = chunk_size as u16;
            }

            entries[current_entry].paddr = current_paddr;
            entries[current_entry].end_of_table = 0;

            current_paddr = current_paddr + chunk_size;
            size_remaining -= chunk_size;
            current_entry += 1;
        }

        // mark the last entry as the end of the table
        entries[current_entry - 1].end_of_table = 0x8000;
    }
}

impl Drop for PRDT {
    fn drop(&mut self) {
        // free the allocated memory for the PRDT table
        let task_id = get_current_id();
        unmap_memory_for_task(task_id, self.table_vaddr, 0x1000);
    }
}

/// An ATA Channel is a single bus with its own set of status and control ports.
/// The controller driver will use these ports to communicate with the drives.
/// Each channel may also have an IRQ line associated with it, which is used to
/// avoid polling for status updates.
pub struct AtaChannel {
    pub base_port: u16,
    pub control_port: u16,
    pub bus_master_port: Option<u16>,
    pub irq_handle: Option<Handle>,
}

impl AtaChannel {
    /// Wait for the controller to have a result. If an IRQ handle is provided,
    /// it can efficiently wait for the interrupt to trigger; otherwise it will
    /// loop in a yielding spin-lock until the status register is no longer busy.
    pub fn wait_for_update(&self) -> Result<(), ()> {
        let status_port = Port::new(self.base_port + 7);
        loop {
            let status = status_port.read_u8();
            if status & 0x01 != 0 {
                // error bit is set, return an error
                return Err(());
            }
            if status & 0x80 == 0 {
                // controller is no longer busy
                return Ok(());
            }

            if let Some(handle) = self.irq_handle {
                // wait for the IRQ to be triggered
                let _ = read_sync(handle, &mut [], 0);
                let _ = write_sync(handle, &[1u8], 0);
            } else {
                yield_coop();
            }
        }
    }

    pub fn identify(&self) -> [Option<DiskProperties>; 2] {
        let drives = [DriveSelect::Primary, DriveSelect::Secondary];

        let mut read_buffer: [u16; 256] = [0; 256];

        drives.map(|drive| {
            // send IDENTIFY command
            Port::new(self.base_port + 6).write_u8(0xa0 | drive as u8);
            sleep(1);
            // reset sector count and LBA registers
            Port::new(self.base_port + 2).write_u8(0);
            Port::new(self.base_port + 3).write_u8(0);
            Port::new(self.base_port + 4).write_u8(0);
            Port::new(self.base_port + 5).write_u8(0);

            Port::new(self.base_port + 7).write_u8(AtaCommand::Identify as u8);
            sleep(1);

            if Port::new(self.base_port + 7).read_u8() == 0 {
                return None;
            }

            let disk_type = match self.wait_for_update() {
                Ok(_) => DiskType::PATA,
                Err(_) => {
                    let sig_low = Port::new(self.base_port + 4).read_u8();
                    let sig_high = Port::new(self.base_port + 5).read_u8();
                    match (sig_low, sig_high) {
                        (0x14, 0xeb) => {
                            // ATAPI
                            DiskType::ATAPI
                        }
                        _ => return None,
                    }
                }
            };

            if let DiskType::ATAPI = disk_type {
                // send IDENTIFY PACKET DEVICE command instead
                Port::new(self.base_port + 7).write_u8(AtaCommand::IdentifyPacketDevice as u8);
                sleep(1);
            }

            let data_port = Port::new(self.base_port);
            for i in 0..read_buffer.len() {
                read_buffer[i] = data_port.read_u16();
            }

            let sectors = (read_buffer[60] as u32) | ((read_buffer[61] as u32) << 8);
            let serial = extract_ata_string(&read_buffer[10..20]);

            return Some(DiskProperties {
                disk_type,
                sectors,
                location: drive,
                serial,
            });
        })
    }

    pub fn read(
        &self,
        drive: DriveSelect,
        first_sector: u32,
        buffer: &mut [u8],
    ) -> Result<u32, ()> {
        if buffer.len() % SECTOR_SIZE != 0 {
            crate::kprintln!(
                "ATA READ: Buffer must be divisible by sector size ({})",
                SECTOR_SIZE
            );
            return Err(());
        }
        match self.bus_master_port {
            Some(port) => {
                // Use DMA transfer if bus mastering is available
                crate::kprintln!("ATA READ: Using DMA transfer");
                return self.read_dma(drive, first_sector, buffer);
            }
            None => {
                // Use PIO transfer
                crate::kprintln!("ATA READ: Using PIO transfer");
                return self.read_pio(drive, first_sector, buffer);
            }
        }
    }

    pub fn dma_transfer(
        &self,
        drive: DriveSelect,
        first_sector: u32,
        buffer: &mut [u8],
        is_write: bool,
    ) -> Result<u32, ()> {
        let prdt = PRDT::new();
        let buffer_vaddr = VirtualAddress::new(buffer.as_ptr() as u32);
        let buffer_paddr = get_current_physical_address(buffer_vaddr).unwrap();
        prdt.set_buffer(buffer_paddr, buffer.len() as u32);

        let bus_master_port = self.bus_master_port.unwrap();
        Port::new(bus_master_port).write_u8(0);
        let flags = Port::new(bus_master_port + 2).read_u8();
        // clear irq and error bits by writing them
        Port::new(bus_master_port + 2).write_u8(flags | 6);
        Port::new(bus_master_port + 4).write_u32(prdt.table_paddr.as_u32());

        Port::new(self.base_port + 6).write_u8(0xa0 | drive as u8);
        sleep(1);

        let status_port = Port::new(self.base_port + 7);
        while status_port.read_u8() & 0x80 != 0 {
            yield_coop();
        }

        crate::kprintln!("DMA Ready, read from sector {}", first_sector);
        let sector_count = buffer.len() as u32 / 512;
        Port::new(self.base_port + 2).write_u8(sector_count as u8);
        Port::new(self.base_port + 3).write_u8(first_sector as u8);
        Port::new(self.base_port + 4).write_u8((first_sector >> 8) as u8);
        Port::new(self.base_port + 5).write_u8((first_sector >> 16) as u8);
        Port::new(self.base_port + 6)
            .write_u8(0xe0 | drive as u8 | ((first_sector >> 24) & 0x0f) as u8);

        Port::new(self.base_port + 7).write_u8(if is_write {
            AtaCommand::WriteDMA as u8
        } else {
            AtaCommand::ReadDMA as u8
        });

        // actually start the DMA
        let mut dma_command = 1;
        if !is_write {
            dma_command |= 8
        };

        Port::new(bus_master_port).write_u8(dma_command);

        loop {
            if let Some(handle) = self.irq_handle {
                // wait for the IRQ to be triggered
                let _ = read_sync(handle, &mut [], 0);
                let _ = write_sync(handle, &[1u8], 0);
            }
            let status = Port::new(bus_master_port + 2).read_u8();
            if status & 0x04 != 0 {
                // transfer is complete
                break;
            }
            if status & 0x02 != 0 {
                Port::new(bus_master_port).write_u8(0);
                let err = Port::new(self.base_port + 1).read_u8();
                crate::kprintln!(
                    "ATA READ: DMA transfer failed with status: {:02x}, err: {:X}",
                    status,
                    err
                );
                return Err(());
            }
        }
        // end dma
        Port::new(bus_master_port).write_u8(0);
        // clear interrupt
        Port::new(bus_master_port + 2).write_u8(4);

        Ok(buffer.len() as u32)
    }

    fn read_dma(
        &self,
        drive: DriveSelect,
        first_sector: u32,
        buffer: &mut [u8],
    ) -> Result<u32, ()> {
        unsafe {
            core::ptr::read_volatile(buffer.as_ptr());
        }
        self.dma_transfer(drive, first_sector, buffer, false)
    }

    fn read_pio(
        &self,
        drive: DriveSelect,
        first_sector: u32,
        buffer: &mut [u8],
    ) -> Result<u32, ()> {
        if first_sector > 0x00ffffff {
            crate::kprintln!("ATA READ: PIO transfer with >24bits not supported yet");
            return Err(());
        }

        let sectors = (buffer.len() + SECTOR_SIZE - 1) / SECTOR_SIZE;

        if sectors > 256 {
            crate::kprintln!("ATA READ: PIO can only transfer 256 sectors at a time");
            return Err(());
        }

        Port::new(self.base_port + 6).write_u8(0xe0 | drive as u8);
        Port::new(self.base_port + 2).write_u8(if sectors >= 256 { 0 } else { sectors as u8 });
        Port::new(self.base_port + 3).write_u8(first_sector as u8);
        Port::new(self.base_port + 4).write_u8((first_sector >> 8) as u8);
        Port::new(self.base_port + 5).write_u8((first_sector >> 16) as u8);

        Port::new(self.base_port + 7).write_u8(AtaCommand::ReadSectors as u8);

        for sector in 0..sectors {
            let read_start = sector * SECTOR_SIZE;
            self.wait_for_update()?;
            for i in 0..256 {
                // ATA spec suggests reading one word at a time
                let data = Port::new(self.base_port).read_u16();
                buffer[read_start + i * 2 + 0] = data as u8;
                buffer[read_start + i * 2 + 1] = (data >> 8) as u8;
            }
        }

        Ok(sectors as u32)
    }
}

/// Stores the important traits passed back from an IDENTIFY command
pub struct DiskProperties {
    pub disk_type: DiskType,
    pub sectors: u32,
    pub location: DriveSelect,
    pub serial: String,
}

#[derive(Copy, Clone)]
pub enum DiskType {
    PATA,
    ATAPI,
    SATA,
}

impl core::fmt::Display for DiskProperties {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.disk_type {
            DiskType::PATA => f.write_fmt(format_args!(
                "ATA Disk \"{}\", {} Bytes",
                self.serial,
                self.sectors * 512
            )),
            DiskType::ATAPI => f.write_fmt(format_args!("ATAPI Disk \"{}\"", self.serial)),
            DiskType::SATA => f.write_fmt(format_args!("SATA Disk \"{}\"", self.serial)),
        }
    }
}
