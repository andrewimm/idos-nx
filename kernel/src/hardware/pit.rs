use crate::arch::port::Port;

/// The Programmable Interval Timer provides a number of configurable timers to
/// produce regular interrupts, update other connected hardware, or send a
/// signal to the PC Speaker.
/// Channel 0 triggers a hardware interrupt on the rising edge of its signal.
/// We use this interrupt to perform regular kernel updates.
pub struct PIT {
    pub channel_0: Port,
    pub command_register: Port,
}

impl PIT {
    pub const fn new() -> Self {
        Self {
            channel_0: Port::new(0x40),
            command_register: Port::new(0x43),
        }
    }

    pub fn set_divider(&mut self, div: u16) {
        // Disable BCD; set Mode 3 (Square Wave Generator); set access on
        // channel 0 to low, then high
        self.command_register.write_u8(0x36);
        // With the current access mode, write the low byte followed by the
        // high byte
        self.channel_0.write_u8((div & 0xff) as u8);
        self.channel_0.write_u8((div >> 8) as u8);
    }
}

