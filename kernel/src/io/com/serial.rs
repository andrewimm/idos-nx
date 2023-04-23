use crate::arch::port::Port;

const STATUS_TRANSMIT_BUFFER_EMPTY: u8 = 1 << 5;
const STATUS_DATA_READY: u8 = 1;

#[allow(dead_code)]
pub struct SerialPort {
    /// Writing to data sends to the transmit buffer, reading pulls from the
    /// receive buffer
    data: Port,
    /// Bitmap enabling various interrupts when serial state changes.
    ///   Bit 0 - Triggered when data available
    ///   Bit 1 - Triggered when transmit buffer is empty
    ///   Bit 2 - Triggered on error
    ///   Bit 3 - Triggered on status change
    interrupt_enable: Port,
    /// Reading from this port is used to identify the current interrupt, as
    /// well as properties of the UART device.
    /// Writing to it changes how the buffers behave
    fifo_control: Port,
    /// Determines the behavior and format of data on the wire
    line_control: Port,
    /// Gives direct control of the hardware transmitting and receiving data
    modem_control: Port,
    /// 
    line_status: Port,
    modem_status: Port,
}

impl SerialPort {
    pub fn new(base_port: u16) -> Self {
        Self {
            data:               Port::new(base_port),
            interrupt_enable:   Port::new(base_port + 1),
            fifo_control:       Port::new(base_port + 2),
            line_control:       Port::new(base_port + 3),
            modem_control:      Port::new(base_port + 4),
            line_status:        Port::new(base_port + 5),
            modem_status:       Port::new(base_port + 6),
        }
    }

    pub fn init(&self) {
        // disable interrupts until we get them working
        self.interrupt_enable.write_u8(0);

        // Enable divisor latch access, allowing the baud rate to be changed
        self.line_control.write_u8(0x80);
        // With DLAB enabled, the data register accesses the low 8 bits of the
        // internal divisor, and the interrupt register accesses the high bits
        self.data.write_u8(0x03); // 115200 / 3 = 38,400 baud
        self.interrupt_enable.write_u8(0);

        // Set a standard 8n1 protocol: 8 bits, no parity, 1 stop bit
       self.line_control.write_u8(0x03);

       // Enable FIFO buffers: set the highest buffer size, clear the buffers,
       // and enable them.
       self.fifo_control.write_u8(0xc7);

       // Enable Aux Output 2 so that interrupts can work; 
       self.modem_control.write_u8(0x08);

       // Enable interrupt for data available
       self.interrupt_enable.write_u8(1);
    }
    
    pub fn is_transmitting(&self) -> bool {
        (self.line_status.read_u8() & STATUS_TRANSMIT_BUFFER_EMPTY) == 0
    }

    pub fn send_byte(&self, byte: u8) {
        while self.is_transmitting() {}
        self.data.write_u8(byte);
    }

    pub fn has_data(&self) -> bool {
        (self.line_status.read_u8() & STATUS_DATA_READY) != 0
    }

    pub fn read_byte(&self) -> Option<u8> {
        if self.has_data() {
            Some(self.data.read_u8())
        } else {
            None
        }
    }
}

impl core::fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            self.send_byte(byte);
        }
        Ok(())
    }
}
