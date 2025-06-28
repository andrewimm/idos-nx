use crate::{
    memory::address::VirtualAddress,
    task::{
        actions::memory::{map_memory, unmap_memory_for_task},
        memory::MemoryBacking,
        switching::get_current_id,
    },
};

use core::arch::asm;

pub struct LocalAPIC {
    pub address: VirtualAddress,
}

impl LocalAPIC {
    pub fn new(address: VirtualAddress) -> Self {
        Self { address }
    }

    pub fn set_icr(&self, high: u32, low: u32) {
        let icr_high = (self.address + 0x310).as_ptr_mut::<u32>();
        let icr_low = (self.address + 0x300).as_ptr_mut::<u32>();

        unsafe {
            core::ptr::write_volatile(icr_high, high);

            core::ptr::write_volatile(icr_low, low);

            loop {
                // wait for the interrupt to send
                let status = core::ptr::read_volatile(icr_low);
                if (status & 0x1000) == 0 {
                    break;
                }
                asm!("pause");
            }
        }
    }

    pub fn broadcast_ipi(&self, vector: u8) {
        self.set_icr(0, (3 << 18) | (vector as u32));
    }

    pub fn enable(&self) {
        let spurious_register = (self.address + 0xf0).as_ptr_mut::<u32>();
        unsafe { core::ptr::write_volatile(spurious_register, 0x1ff) };
    }

    pub fn eoi(&self) {
        let eoi_register = (self.address + 0xb0).as_ptr_mut::<u32>();
        unsafe { core::ptr::write_volatile(eoi_register, 0) };
    }
}
