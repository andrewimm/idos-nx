use alloc::vec::Vec;

use super::devices::{BaseAddressRegister, PciDevice};
use crate::arch::port::Port;

pub fn read_config_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let config_address = 0x80000000 | // enable
        ((bus as u32) << 16) |
        ((device as u32) << 11) |
        ((function as u32) << 8) |
        ((offset as u32) & 0xfc);

    Port::new(0xcf8).write_u32(config_address);

    Port::new(0xcfc).read_u32()
}

pub fn write_config_u32(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    let config_address = 0x80000000 | // enable
        ((bus as u32) << 16) |
        ((device as u32) << 11) |
        ((function as u32) << 8) |
        ((offset as u32) & 0xfc);

    Port::new(0xcf8).write_u32(config_address);

    Port::new(0xcfc).write_u32(value);
}

pub fn get_bar(bus: u8, device: u8, function: u8, index: u8) -> BaseAddressRegister {
    let offset = 0x10 + index * 4;
    let value = read_config_u32(bus, device, function, offset);

    BaseAddressRegister(value)
}

pub fn get_vendor_id(bus: u8, device: u8, function: u8) -> u16 {
    read_config_u32(bus, device, function, 0) as u16
}

pub fn get_header_type(bus: u8, device: u8, function: u8) -> u8 {
    let config = read_config_u32(bus, device, function, 0xc);
    (config >> 16) as u8
}

pub fn add_device(devices: &mut Vec<PciDevice>, bus: u8, device: u8, function: u8) {
    let device = PciDevice::read_from_bus(bus, device, function);
    crate::kprint!(" |- {}\n", device);
    if let Some(irq) = device.irq {
        crate::kprint!(" |    IRQ: {}\n", irq);
    }
    crate::kprint!(" |    ProgIf: {:08b}\n", device.programming_interface);
    for i in 0..6 {
        if let Some(bar) = device.bar[i] {
            crate::kprint!(" |    BAR{}: {}\n", i, bar);
        }
    }
    devices.push(device);
}

pub fn lookup_device(devices: &mut Vec<PciDevice>, bus: u8, device: u8) {
    let vendor = get_vendor_id(bus, device, 0);
    if vendor == 0xffff {
        return;
    }

    let header_type = get_header_type(bus, device, 0);

    add_device(devices, bus, device, 0);
    if header_type & 0x80 != 0 {
        for function in 1..8 {
            let mf_vendor = get_vendor_id(bus, device, function);
            if mf_vendor == 0xffff {
                continue;
            }
            add_device(devices, bus, device, function);
        }
    }
}

pub fn enumerate(devices: &mut Vec<PciDevice>) {
    crate::kprint!("PCI Devices:\n");

    for bus in 0..=255 {
        for device in 0..32 {
            lookup_device(devices, bus, device);
        }
    }
}
