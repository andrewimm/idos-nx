pub mod controller;
pub mod dev;
pub mod protocol;

use crate::arch::port::Port;
use crate::task::actions::{yield_coop, sleep};
use protocol::{AtaCommand, extract_ata_string};

#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum DriveSelect {
    Primary = 0xa0,
    Secondary = 0xb0,
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

    /// Memoize the last drive selected, to avoid unnecessary PIO
    pub drive_select: Option<DriveSelect>,
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

            drive_select: None,
        }
    }

    pub fn select(&mut self, select: DriveSelect) {
        if self.drive_select != Some(select) {
            self.drive_register.write_u8(select as u8);
            self.drive_select = Some(select);
            sleep(1);
        }
    }

    pub fn identify(&mut self) {
        let mut buffer: [u16; 256] = [0; 256];
        self.sector_count.write_u8(0);
        self.lba_low.write_u8(0);
        self.lba_mid.write_u8(0);
        self.lba_high.write_u8(0);

        self.command_status.write_u8(AtaCommand::Identify as u8);
        sleep(1);

        if self.command_status.read_u8() == 0 {
            crate::kprint!("DISK NOT FOUND\n");
            return;
        }

        loop {
            let status = self.command_status.read_u8();
            if status & 0x01 != 0 {
                crate::kprint!("ATA IDENTIFY ERR\n");
                return;
            }
            if status & 0x80 == 0 {
                break;
            }
            yield_coop();
        }

        let mut read_index = 0;
        while read_index < buffer.len() {
            buffer[read_index] = self.data.read_u16();
            read_index += 1;
        }

        let serial = extract_ata_string(&buffer[10..20]);

        crate::kprint!("    SERIAL NO: {}\n", serial);

        let addressable_sectors =
            (buffer[60] as u32) |
            ((buffer[61] as u32) << 8);

        crate::kprint!("    SECTORS: {}\n", addressable_sectors);

        crate::kprint!("IDENTIFY DONE\n\n");
    }
}

