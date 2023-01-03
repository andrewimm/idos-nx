use crate::arch::port::Port;

/// The Programmable Interrupt Chip handles all of the hardware interrupts that
/// come into the CPU. Devices send a signal to the chip to generate an
/// interrupt that can be handled in software.
/// On the PC architecture, there are actually two PIC chips chained together.
/// The primary handles the first 8 interrupts, and the secondary adds another
/// 8.
pub struct PIC {
    primary_command: Port,
    primary_data: Port,
    secondary_command: Port,
    secondary_data: Port,
}

impl PIC {
    pub const fn new() -> Self {
        Self {
            primary_command: Port::new(0x20),
            primary_data: Port::new(0x21),
            secondary_command: Port::new(0xa0),
            secondary_data: Port::new(0xa1),
        }
    }

    /// Initialize the pair of PIC chips to work 
    pub fn init(&self) {
        // Initialize with Command Word 1, mark ICW4 as needed
        self.primary_command.write_u8(0x11);
        self.secondary_command.write_u8(0x11);
        // ICW2: set the primary PIC's IDT offset to 0x30
        self.primary_data.write_u8(0x30);
        // ICW2: set the secondary PIC's IDT offset to 0x38
        self.secondary_data.write_u8(0x38);
        // ICW3: tell the primary chip that there is a secondary on line 2 (bitmap)
        self.primary_data.write_u8(0x04);
        // ICW3: tell the secondary that it is chained to IRQ 2
        self.secondary_data.write_u8(0x02);
        // ICW4: Use 8086 mode
        self.primary_data.write_u8(0x01);
        self.secondary_data.write_u8(0x01);
    }

    pub fn acknowledge_interrupt(&self, irq: u8) {
        if irq >= 8 {
            self.secondary_command.write_u8(0x20);
        }
        // regardless of whether the interrupt happened on the primary or
        // secondary, the primary still needs to be cleared
        self.primary_command.write_u8(0x20);
    }

    /// In order to detect spurious interrupts, the kernel needs to read the
    /// interrupts currently in service. This will allow it to account for
    /// interrupts that were triggered, and then removed before serviced.
    pub fn get_interrupts_in_service(&self) -> u16 {
        self.primary_command.write_u8(0x0b);
        self.secondary_command.write_u8(0x0b);

        let low = self.primary_command.read_u8() as u16;
        let high = (self.secondary_command.read_u8() as u16) << 8;

        high | low
    }
}

