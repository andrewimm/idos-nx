use core::arch::asm;

/// The x86 processor family has two address spaces: memory and IO
/// IO addresses are called Ports, and are used to interface directly with
/// low-level hardware. Since this is CPU bound, this has been replaced over
/// time with bus systems that are available via memory address, but it remains
/// valuable for initializing the system and working with the lowest level
/// elements in the PC.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Port(u16);

impl Port {
    pub const fn new(number: u16) -> Self {
        Self(number)
    }

    pub fn write_u8(&self, value: u8) {
        unsafe {
            // writing to a variable port is only possible through the DX and AL
            // registers
            asm!(
                "out dx, al",
                in("dx") self.0,
                in("al") value,
            );
        }
    }

    pub fn read_u8(&self) -> u8 {
        let value: u8;
        unsafe {
            asm!(
                "in al, dx",
                out("al") value,
                in("dx") self.0,
            );
        }
        value
    }
}
