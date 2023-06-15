use crate::{arch::port::Port, interrupts::pic::install_interrupt_handler};


pub struct PS2Controller {
    data: Port,
    command: Port,
}

impl PS2Controller {
    pub fn new() -> Self {
        Self {
            data: Port::new(0x60),
            command: Port::new(0x64),
        }
    }

    pub fn output_buffer_ready(&self) -> bool {
        let status = self.command.read_u8();
        status & 1 != 0
    }

    pub fn input_buffer_ready(&self) -> bool {
        let status = self.command.read_u8();
        status & 2 == 0
    }

    pub fn init(&self) {
        crate::kprint!("Set up PS/2\n");

        // TODO: actually configure and query the device
        // Every time I try to reconfigure the controller, interrupts get
        // disabled and things break, so I give up...
        install_interrupt_handler(1, handle_interrupt);
        install_interrupt_handler(12, handle_interrupt);
    }
}

pub fn handle_interrupt(_irq: u32) {
    crate::kprint!("!");
    let _data = Port::new(0x60).read_u8();
}

