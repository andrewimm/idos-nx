use alloc::string::String;

use crate::arch::port::Port;
use crate::task::actions::{yield_coop, sleep};
use super::protocol::{AtaCommand, extract_ata_string};

pub const SECTOR_SIZE: u32 = 512;

#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum DriveSelect {
    Primary = 0x00,
    Secondary = 0x10,
}

pub struct AtaController {
    pub data: Port,
    pub error: Port,
    pub sector_count: Port,
    pub lba_low: Port,
    pub lba_mid: Port,
    pub lba_high: Port,
    pub drive_register: Port,
    pub command_status: Port,

    pub device_control: Port,
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
            DiskType::PATA =>
                f.write_fmt(format_args!("ATA Disk \"{}\", {} Bytes", self.serial, self.sectors * 512)),
            DiskType::ATAPI =>
                f.write_fmt(format_args!("ATAPI Disk \"{}\"", self.serial)),
            DiskType::SATA =>
                f.write_fmt(format_args!("SATA Disk \"{}\"", self.serial)),
        }
    }
}

impl AtaController {
    pub fn new(base_port: u16, device_control_port: u16) -> Self {
        Self {
            data: Port::new(base_port),
            error: Port::new(base_port + 1),
            sector_count: Port::new(base_port + 2),
            lba_low: Port::new(base_port + 3),
            lba_mid: Port::new(base_port + 4),
            lba_high: Port::new(base_port + 5),
            drive_register: Port::new(base_port + 6),
            command_status: Port::new(base_port + 7),

            device_control: Port::new(device_control_port),
        }
    }

    pub fn poll(&self) -> Result<(), ()> {
        loop {
            let status = self.command_status.read_u8();
            if status & 0x01 != 0 {
                return Err(());
            }
            if status & 0x80 == 0 {
                return Ok(());
            }
            yield_coop();
        }
    }

    /// Check for up to two ATA drives attached to this Controller.
    pub fn identify(&self) -> [Option<DiskProperties>; 2] {
        let drives = [DriveSelect::Primary, DriveSelect::Secondary];

        let mut buffer: [u16; 256] = [0; 256];

        drives.map(|drive| {
            // send IDENTIFY command
            self.drive_register.write_u8(0xa0 | drive as u8);
            sleep(1);
            self.sector_count.write_u8(0);
            self.lba_low.write_u8(0);
            self.lba_mid.write_u8(0);
            self.lba_high.write_u8(0);

            self.command_status.write_u8(AtaCommand::Identify as u8);
            sleep(1);

            if self.command_status.read_u8() == 0 {
                return None;
            }

            let disk_type = if let Err(_) = self.poll() {
                let sig_low = self.lba_mid.read_u8();
                let sig_high = self.lba_high.read_u8();
                match (sig_low, sig_high) {
                    (0x14, 0xeb) => { // ATAPI
                        crate::kprint!("!!! FOUND ATAPI !!!\n");
                        DiskType::ATAPI
                    },
                    _ => return None,
                }
            } else {
                DiskType::PATA
            };
            
            if let DiskType::ATAPI = disk_type {
                // send IDENTIFY PACKET DEVICE command instead
                self.command_status.write_u8(AtaCommand::IdentifyPacketDevice as u8);
                sleep(1);
            }

            for i in 0..buffer.len() {
                buffer[i] = self.data.read_u16();
            }

            let sectors =
                (buffer[60] as u32) |
                ((buffer[61] as u32) << 8);

            let serial = extract_ata_string(&buffer[10..20]);

            return Some(
                DiskProperties {
                    disk_type,
                    sectors,
                    location: drive,
                    serial,
                }
            );
        })
    }

    pub fn read_sectors(&self, drive: DriveSelect, first_sector: u32, buffer: &mut [u8]) -> Result<u32, ()> {
        if (buffer.len() as u32) % SECTOR_SIZE != 0 {
            panic!("ATA READ: Buffer must be divisible by sector size ({})", SECTOR_SIZE);
        }
        if first_sector > 0x00ffffff {
            panic!("ATA READ: PIO transfer with >24 bits not supported yet");
        }

        let sectors = (buffer.len() as u32 + SECTOR_SIZE - 1) / SECTOR_SIZE;

        if sectors > 256 {
            panic!("ATA READ: PIO can only transfer 256 sectors at a time");
        }

        self.drive_register.write_u8(0xe0 | drive as u8);
        self.sector_count.write_u8(
            if sectors >= 256 {
                0
            } else {
                sectors as u8
            }
        );
        self.lba_low.write_u8(first_sector as u8);
        self.lba_mid.write_u8((first_sector >> 8) as u8);
        self.lba_high.write_u8((first_sector >> 16) as u8);

        self.command_status.write_u8(AtaCommand::ReadSectors as u8);

        for sector in 0..sectors {
            // need at least 400ns for the status register to be correct
            sleep(1);
            let read_start = (sector * SECTOR_SIZE) as usize;
            self.poll();
            for i in 0..256 {
                // ATA spec suggests reading one word at a time
                let data = self.data.read_u16();
                buffer[read_start + i * 2] = data as u8;
                buffer[read_start + i * 2 + 1] = (data >> 8) as u8;
            }
        }

        Ok(sectors)
    }
}
