use crate::memory::address::VirtualAddress;

/// Provides access for an Intel e1000 Ethernet Controller
pub struct E1000Controller {
    io: ControllerIO,
}

impl E1000Controller {
    pub fn new(mmio_base: VirtualAddress) -> Self {
        Self {
            io: ControllerIO::MMIO(MMIO { base: mmio_base }),

        }
    }

    pub fn get_mac_address(&self) -> [u8; 6] {
        // MAC is split between two different registers
        let ral = self.io.get_response(0x5400);
        let rah = self.io.get_response(0x5404);
        [
            ral as u8,
            (ral >> 8) as u8,
            (ral >> 16) as u8,
            (ral >> 24) as u8,
            rah as u8,
            (rah >> 8) as u8,
        ]
    }
}

enum ControllerIO {
    MMIO(MMIO),
    PIO(PIO),
}

impl ControllerIO {
    pub fn write_command(&self, address: u16, command: u32) {
        match self {
            ControllerIO::MMIO(mmio) => {
                let ptr = mmio.get_pointer(address);
                unsafe { core::ptr::write_volatile(ptr, command); }
            },
            ControllerIO::PIO(pio) => {
                panic!("PIO controller not implemented yet");
            },
        }
    }

    pub fn get_response(&self, address: u16) -> u32 {
        match self {
            ControllerIO::MMIO(mmio) => {
                let ptr = mmio.get_pointer(address);
                unsafe { core::ptr::read_volatile(ptr) }
            },
            ControllerIO::PIO(pio) => {
                panic!("PIO controller not implemented yet");
            },
        }
    }
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
