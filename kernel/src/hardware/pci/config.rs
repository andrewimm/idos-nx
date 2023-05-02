use crate::arch::port::Port;
use super::super::devicetree::{self, DeviceID, DeviceNode, DeviceNodeType, DeviceTree};

pub struct BaseAddressRegister(u32);

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
        self.0 == 0
    }
}

pub fn read_config_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    // The IO port returns 32 bits at a time, at offset multiples of 4
    // The actual value of offset selects the location of the 16 bits to read
    // from within the returned value.
    let config_value = read_config_u32(bus, device, function, offset);
    let shift = (offset as usize & 2) * 8;
    (config_value >> shift) as u16
}

pub fn read_config_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let config_address =
        0x80000000 | // enable
        ((bus as u32) << 16) |
        ((device as u32) << 11) |
        ((function as u32) << 8) |
        ((offset as u32) & 0xfc);

    Port::new(0xcf8).write_u32(config_address);

    Port::new(0xcfc).read_u32()
}

pub fn get_vendor_id(bus: u8, device: u8, function: u8) -> u16 {
    read_config_u16(bus, device, function, 0)
}

pub fn get_device_id(bus: u8, device: u8, function: u8) -> u16 {
    read_config_u16(bus, device, function, 2)
}

/// Return a u16 containing both the class code and subclass
pub fn get_class(bus: u8, device: u8, function: u8) -> u16 {
    read_config_u16(bus, device, function, 0xa)
}

pub fn get_programming_interface(bus: u8, device: u8, function: u8) -> u8 {
    (read_config_u16(bus, device, function, 8) >> 8) as u8
}

pub fn get_header_type(bus: u8, device: u8, function: u8) -> u8 {
    read_config_u16(bus, device, function, 0xe) as u8
}

pub fn get_bar(bus: u8, device: u8, function: u8, index: u8) -> BaseAddressRegister {
    let offset = 0x10 + index * 4;
    let value = read_config_u32(bus, device, function, offset);

    BaseAddressRegister(value)
}

/// Return a tuple containing the interrupt line and interrupt pin
pub fn get_interrupt_data(bus: u8, device: u8, function: u8) -> (u8, u8) {
    let interrupt_data = read_config_u16(bus, device, function, 0x3c);
    (interrupt_data as u8, (interrupt_data >> 8) as u8)
}

pub fn print_info(bus: u8, device: u8, function: u8) {
    let header_type = get_header_type(bus, device, function);
    let class = get_class(bus, device, function);
    let class_code = (class >> 8) as u8;
    let subclass = class as u8;
    let prog_if = get_programming_interface(bus, device, function);
    crate::kprint!("  Device ({:X}:{:X}:{:X}): {:X} / {:X} / {:X}", bus, device, function, class_code, subclass, prog_if);
    if header_type & 0x80 != 0 {
        crate::kprint!(" (MF)\n");
        return;
    }
    crate::kprint!("\n");
    let (interrupt_line, interrupt_pin) = get_interrupt_data(bus, device, function);
    if interrupt_line != 0xff && interrupt_pin != 0 {
        let pin = match interrupt_pin {
            1 => "INTA",
            2 => "INTB",
            3 => "INTC",
            4 => "INTD",
            _ => "NONE",
        };
        crate::kprint!("    INT: {:X} (PIN {})\n", interrupt_line, pin);
    }
    for i in 0..6 {
        let bar = get_bar(bus, device, function, i);
        if bar.is_empty() {
            continue;
        }
        let bar_type = if bar.is_io() {
            "IO"
        } else {
            "MEM"
        };
        crate::kprint!("    BAR {} - {}: {:X}\n", i, bar_type, bar.get_address());
    }
}

pub fn add_device(device_tree: &mut DeviceTree, parent: DeviceID, bus: u8, device: u8, function: u8) -> DeviceID {
    let class = get_class(bus, device, function);
    let class_code = (class >> 8) as u8;
    let subclass = class as u8;

    crate::kprint!("  ({:X}:{:X}:{:X}): {:X} / {:X}", bus, device, function, class_code, subclass);

    let node = if class_code == 1 {
        // mass storage
        if subclass == 1 {
            crate::kprint!(" - IDE Bus");
            DeviceNode::new(
                DeviceNodeType::Bus(devicetree::bus::BusType::IDE)
            )
        } else {
            DeviceNode::new(DeviceNodeType::Unknown)
        }
    } else if class_code == 2 {
        // network controller
        if subclass == 0 {
            crate::kprint!(" - Ethernet");
        }
        DeviceNode::new(DeviceNodeType::Unknown)
    } else if class_code == 3 {
        // display controller
        if subclass == 0 {
            crate::kprint!(" - VGA");
        }
        DeviceNode::new(DeviceNodeType::Unknown)
    } else if class_code == 6 {
        // bridge
        if subclass == 0 {
            crate::kprint!(" - PCI Bridge");
            DeviceNode::new(
                DeviceNodeType::Bus(devicetree::bus::BusType::IDE)
            )
        } else if subclass == 1 {
            crate::kprint!(" - ISA Bridge");
            DeviceNode::new(
                DeviceNodeType::Bus(devicetree::bus::BusType::ISA)
            )
        } else {
            DeviceNode::new(DeviceNodeType::Unknown)
        }
    } else {
        DeviceNode::new(DeviceNodeType::Unknown)
    };

    crate::kprint!("\n");

    device_tree.insert_node(parent, node)
}

pub fn lookup_device(device_tree: &mut DeviceTree, bus: u8, device: u8) {
    let vendor = get_vendor_id(bus, device, 0);
    if vendor == 0xffff {
        return;
    }

    let header_type = get_header_type(bus, device, 0);

    let node_id = add_device(device_tree, device_tree.get_root(), bus, device, 0);
    if header_type & 0x80 != 0 {
        for function in 1..8 {
            let mf_vendor = get_vendor_id(bus, device, function);
            if vendor == 0xffff {
                continue;
            }
            //print_info(bus, device, function);
            add_device(device_tree, node_id, bus, device, function);
        }
    }
    crate::kprint!("\n");
}

pub fn enumerate(device_tree: &mut DeviceTree) {
    crate::kprint!("PCI Devices:\n");

    for bus in 0..=255 {
        for device in 0..32 {
            lookup_device(device_tree, bus, device);
        }
    }
}
