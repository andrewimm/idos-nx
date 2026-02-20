use crate::arch::port::Port;

/// The PIT's base oscillator frequency in Hz
pub const PIT_BASE_FREQ: u32 = 1_193_182;

/// The divider value used to configure channel 0's interrupt rate.
/// 11932 gives approximately 100 Hz (1,193,182 / 11932 ≈ 99.998 Hz).
pub const PIT_DIVIDER: u16 = 11932;

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

    /// Read the current countdown value from channel 0.
    /// The PIT counts down from the divider value to 0, then reloads.
    /// By latching the counter, we can read its current value without
    /// disturbing the counting process.
    pub fn read_counter(&self) -> u16 {
        // Latch command for channel 0: channel 0 (bits 7:6 = 00),
        // latch count (bits 5:4 = 00)
        self.command_register.write_u8(0x00);
        // Read low byte then high byte
        let lo = self.channel_0.read_u8() as u16;
        let hi = self.channel_0.read_u8() as u16;
        (hi << 8) | lo
    }
}

