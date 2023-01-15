use crate::arch::port::Port;

pub fn read_config_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let config_address =
        0x80000000 | // enable bit
        ((bus as u32) << 16) |
        ((device as u32) << 11) |
        ((function as u32) << 8) |
        ((offset as u32) & 0xfc);

    Port::new(0xcf8).write_u32(config_address);

    // The IO port returns 32 bits at a time, at offset multiples of 4
    // The actual value of offset selects the location of the 16 bits to read
    // from within the returned value.
    let config_value = Port::new(0xcfc).read_u32();
    let shift = (offset as usize & 2) * 8;
    (config_value >> shift) as u16
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

pub fn lookup_device(bus: u8, device: u8) {
    let vendor = get_vendor_id(bus, device, 0);
    if vendor == 0xffff {
        return;
    }

    let header_type = get_header_type(bus, device, 0);

    let class = get_class(bus, device, 0);
    let class_code = (class >> 8) as u8;
    let subclass = class as u8;
    crate::kprint!("  Device: {:X} / {:X}", class_code, subclass);
    if header_type & 0x80 != 0 {
        crate::kprint!(" (MF)");
    }
    crate::kprint!("\n");
}

pub fn enumerate() {
    crate::kprint!("PCI Devices:\n");

    for bus in 0..=255 {
        for device in 0..32 {
            lookup_device(bus, device);
        }
    }
}
