//! An interface to the low-level Floppy Disk controller, allowing a driver to
//! communicate with the disk drive hardware.
//!
//! Disk access involves sending commands to the controller, and then waiting
//! for an IRQ6 interrupt if the command returns a response. Sending commands
//! involves looping and waiting for some result, and is problematic.
//! Drivers accessing the controller should be aware of this.

use crate::arch::port::Port;

pub struct FloppyDiskController {
    
}

pub fn init() {
    // first, detect the number of drives from the CMOS register
    Port::new(0x70).write_u8(0x10);
    let cmos_value = Port::new(0x71).read_u8();
    let primary_drive = DriveType::from_cmos_value(cmos_value >> 4);
    let secondary_drive = DriveType::from_cmos_value(cmos_value & 0x0f);

    crate::kprint!("Drives Detected:\n");
    crate::kprint!("  Primary:   {:}\n", primary_drive);
    crate::kprint!("  Secondary: {:}\n", secondary_drive);
}

pub fn handle_interrupt(_irq: u8) {
    
}

pub enum DriveType {
    None,
    Capacity360K,
    Capacity1200K,
    Capacity720K,
    Capacity1440K,
    Capacity2880K,
}

impl DriveType {
    pub fn from_cmos_value(value: u8) -> Self {
        match value {
            1 => Self::Capacity360K,
            2 => Self::Capacity720K,
            3 => Self::Capacity1200K,
            4 => Self::Capacity1440K,
            5 => Self::Capacity2880K,
            _ => Self::None,
        }
    }
}

impl core::fmt::Display for DriveType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DriveType::None => f.write_str("Unavailable"),
            DriveType::Capacity360K => f.write_str("360KB 5.25 Disk"),
            DriveType::Capacity1200K => f.write_str("1.2MB 5.25 Disk"),
            DriveType::Capacity720K => f.write_str("720KB 3.5 Disk"),
            DriveType::Capacity1440K => f.write_str("1.44MB 3.5 Disk"),
            DriveType::Capacity2880K => f.write_str("2.88MB 3.5 Disk"),
        }
    }
}
