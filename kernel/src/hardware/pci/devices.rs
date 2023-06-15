use alloc::string::String;
use super::config::{write_config_u32, read_config_u32, get_bar};

#[derive(Clone)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,

    pub vendor_id: u16,
    pub device_id: u16,
    pub command: u16,
    pub status: u16,
    pub programming_interface: u8,
    pub subclass: u8,
    pub class_code: u8,
    pub bar: [Option<BaseAddressRegister>; 6],
    pub irq: Option<u8>,

    pub readable_name: String,
}

impl PciDevice {
    pub fn read_from_bus(bus: u8, device: u8, function: u8) -> Self {
        let id = read_config_u32(bus, device, function, 0);
        let vendor_id = id as u16;
        let device_id = (id >> 16) as u16;
        let command_status = read_config_u32(bus, device, function, 4);
        let command = command_status as u16;
        let status = (command_status >> 16) as u16;
        let codes = read_config_u32(bus, device, function, 8);
        let programming_interface = (codes >> 8) as u8;
        let subclass = (codes >> 16) as u8;
        let class_code = (codes >> 24) as u8;

        let mut bar = [None; 6];
        for i in 0..6 {
            let b = get_bar(bus, device, function, i as u8);
            if b.is_empty() {
                continue;
            }
            bar[i] = Some(b);
        }
        let interrupt = read_config_u32(bus, device, function, 0x3c) as u16;
        let interrupt_line = interrupt as u8;
        let interrupt_pin = (interrupt >> 8) as u8;

        let irq = if interrupt_line == 0xff || interrupt_pin == 0 {
            None
        } else {
            Some(interrupt_line)
        };

        let readable_name = Self::get_name(class_code, subclass);


        Self {
            bus,
            device,
            function,

            vendor_id,
            device_id,
            command,
            status,
            programming_interface,
            subclass,
            class_code,
            bar,
            irq,

            readable_name,
        }
    }

    pub fn get_name(class_code: u8, subclass: u8) -> String {
        if class_code == 1 {
            if subclass == 1 {
                return String::from("IDE Bus");
            }
        } else if class_code == 2 {
            if subclass == 0 {
                return String::from("Ethernet");
            }
        } else if class_code == 3 {
            if subclass == 0 {
                return String::from("VGA");
            }
        } else if class_code == 6 {
            if subclass == 0 {
                return String::from("PCI Bridge");
            }
            if subclass == 1 {
                return String::from("ISA Bridge");
            }
        }

        String::new()
    }

    pub fn enable_bus_master(&self) {
        let command_status = read_config_u32(self.bus, self.device, self.function, 4);
        // add bit 2, enabling bus mastering
        write_config_u32(self.bus, self.device, self.function, 4, command_status | 4);
    }
}

impl core::fmt::Display for PciDevice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = if self.readable_name.is_empty() {
            "Device"
        } else {
            self.readable_name.as_str()
        };
        f.write_fmt(
            format_args!(
                "{} ({:X}:{:X}:{:X}): {:04X}-{:04X}",
                name,
                self.bus,
                self.device,
                self.function,
                self.vendor_id,
                self.device_id,
            )
        )
    }
}

#[derive(Copy, Clone)]
pub struct BaseAddressRegister(pub u32);

impl BaseAddressRegister {
    pub fn get_address(&self) -> u32 {
        if self.is_io() {
            self.0 & 0xfffffffc
        } else {
            self.0 & 0xfffffff0
        }
    }

    pub fn is_io(&self) -> bool {
        self.0 & 1 == 1
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0 ||
        self.0 == 0xffffffff
    }
}

impl core::fmt::Display for BaseAddressRegister {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let bar_type = if self.is_io() {
            "I/O"
        } else {
            "Memory"
        };
        f.write_fmt(format_args!("{} at {:#010X}", bar_type, self.get_address()))
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum DeviceClass {
    Unclassified = 0,
    MassStorage = 1,
    Network = 2,
    Display = 3,
    Multimedia = 4,
    Memory = 5,
    Bridge = 6,
    Communication = 7,
    BaseSystem = 8,
    Input = 9,
    Docking = 0xa,
    Processor = 0xb,
    SerialBus = 0xc,
    Wireless = 0xd,
}

