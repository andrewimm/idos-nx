use core::u8;

use crate::memory::address::VirtualAddress;

/// Provides access for an Intel e1000 Ethernet Controller
pub struct E1000Controller {
    io: ControllerIO,
}

impl E1000Controller {
    pub fn with_mmio(mmio_base: VirtualAddress) -> Self {
        let io = ControllerIO::MMIO(MMIO { base: mmio_base });

        Self {
            io,
        }
    }

    pub fn with_pio() -> Self {
        let io = ControllerIO::PIO(PIO {});

        Self {
            io,
        }
    }

    pub fn write_register(&self, address: u16, command: u32) {
        match &self.io {
            ControllerIO::MMIO(mmio) => {
                let ptr = mmio.get_pointer(address);
                unsafe { core::ptr::write_volatile(ptr, command); }
            },
            ControllerIO::PIO(_pio) => {
                panic!("PIO controller not implemented yet");
            },
        }
    }

    pub fn read_register(&self, address: u16) -> u32 {
        match &self.io {
            ControllerIO::MMIO(mmio) => {
                let ptr = mmio.get_pointer(address);
                unsafe { core::ptr::read_volatile(ptr) }
            },
            ControllerIO::PIO(_pio) => {
                panic!("PIO controller not implemented yet");
            },
        }
    }

    pub fn set_flags(&self, address: u16, flags: u32) {
        let prev = self.read_register(address);
        self.write_register(address, prev | flags);
    }

    pub fn clear_flags(&self, address: u16, flags: u32) {
        let prev = self.read_register(address);
        self.write_register(address, prev & !flags);
    }

    pub fn get_mac_address(&self) -> [u8; 6] {
        // MAC is split between two different registers
        let ral = self.read_register(0x5400);
        let rah = self.read_register(0x5404);
        [
            ral as u8,
            (ral >> 8) as u8,
            (ral >> 16) as u8,
            (ral >> 24) as u8,
            rah as u8,
            (rah >> 8) as u8,
        ]
    }

    pub fn link(&self) {
    }
}

enum ControllerIO {
    MMIO(MMIO),
    PIO(PIO),
}

struct MMIO {
    base: VirtualAddress,
}

impl MMIO {
    pub fn get_pointer(&self, address: u16) -> *mut u32 {
        let pointer_addr = self.base + (address as u32);
        pointer_addr.as_ptr_mut::<u32>()
    }
}

struct PIO {
}



