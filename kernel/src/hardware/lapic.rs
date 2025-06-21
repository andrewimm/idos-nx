use crate::{
    memory::address::PhysicalAddress,
    task::{
        actions::memory::{map_memory, unmap_memory_for_task},
        memory::MemoryBacking,
        switching::get_current_id,
    },
};

use core::arch::asm;

pub struct LocalAPIC {
    address: PhysicalAddress,
}

impl LocalAPIC {
    pub fn new(address: PhysicalAddress) -> Self {
        Self { address }
    }

    pub fn set_icr(&self, high: u32, low: u32) {
        // if this gets called a lot, we should permanently map it
        let apic_mapping = map_memory(None, 0x1000, MemoryBacking::Direct(self.address)).unwrap();

        let icr_high = (apic_mapping + 0x310).as_ptr_mut::<u32>();
        let icr_low = (apic_mapping + 0x300).as_ptr_mut::<u32>();

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

        unmap_memory_for_task(get_current_id(), apic_mapping, 0x1000).unwrap();
    }
}
