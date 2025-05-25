use alloc::string::String;

use super::protocol::{extract_ata_string, AtaCommand};
use crate::arch::port::Port;
use crate::io::handle::Handle;
use crate::task::actions::io::{read_sync, write_sync};
use crate::task::actions::{sleep, yield_coop};

pub const SECTOR_SIZE: usize = 512;

/// Each ATA Channel has up to two connected drives. The channel needs to be
/// told which disk to use before each command.
#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum DriveSelect {
    Primary = 0x00,
    Secondary = 0x10,
}

/// An ATA Channel is a single bus with its own set of status and control ports.
/// The controller driver will use these ports to communicate with the drives.
/// Each channel may also have an IRQ line associated with it, which is used to
/// avoid polling for status updates.
pub struct AtaChannel {
    pub base_port: u16,
    pub control_port: u16,
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

    pub fn read_pio(
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
