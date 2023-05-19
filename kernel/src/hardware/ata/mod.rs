use alloc::boxed::Box;
use alloc::vec::Vec;
use crate::arch::port::Port;
use crate::task::actions::{yield_coop, sleep};

#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum DriveSelect {
    Primary = 0xa0,
    Secondary = 0xb0,
}

#[repr(u8)]
pub enum AtaCommand {
    Identify = 0xec,
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

    pub buffer: Box<[u8]>,
}

impl AtaController {
    pub fn new(base_port: u16, device_control_port: u16) -> Self {
        let mut buffer = Vec::with_capacity(512);
        for i in 0..512 {
            buffer.push(0);
        }
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

            buffer: buffer.into_boxed_slice(),
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
        while read_index < 512 {
            let data = self.data.read_u16();
            self.buffer[read_index] = (data >> 8) as u8;
            self.buffer[read_index + 1] = data as u8;
            read_index += 2;
        }

        let serial = unsafe {
            core::str::from_utf8_unchecked(&self.buffer[20..40])
        };

        crate::kprint!("    SERIAL NO: {}\n", serial);

        crate::kprint!("IDENTIFY DONE\n\n");
    }
}

