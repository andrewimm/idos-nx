use crate::hardware::{pic::PIC, pit::PIT};
use crate::memory::address::PhysicalAddress;
use crate::memory::heap;
use crate::memory::physical::{init_allocator};

extern {
    #[link_name = "__bss_start"]
    static mut label_bss_start: u8;
    #[link_name = "__bss_end"]
    static label_bss_end: u8;
    #[link_name = "__kernel_end"]
    static label_kernel_end: ();
}

/// Zero out the .bss section. Code may assume this area starts as zeroes.
pub unsafe fn zero_bss() {
    let bss_start = &mut label_bss_start as *mut u8;
    let bss_length = (&label_bss_end as *const u8 as usize) - (bss_start as usize); 
    let bss_slice = core::slice::from_raw_parts_mut(bss_start, bss_length);
    for i in 0..bss_slice.len() {
        bss_slice[i] = 0;
    }
}

/// Initialize the GDT, IDT
pub unsafe fn init_cpu_tables() {
    let gdt = &crate::arch::gdt::GDT;
    let gdt_descriptor = &mut crate::arch::gdt::GDTR;
    gdt_descriptor.point_to(gdt);
    gdt_descriptor.load();

    crate::interrupts::idt::init_idt();
}

/// Initialize system memory, enabling virtual memory and paging.
/// Once virtual memory has been enabled, all references to kernel addresses
/// need to be or-ed with 0xc0000000 so that they can correctly point to the
/// kernel in all tasks.
pub unsafe fn init_memory() {
    let allocator_location = &label_kernel_end as *const () as u32;
    let bios_memmap = PhysicalAddress::new(0x1000);
    init_allocator(PhysicalAddress::new(allocator_location), bios_memmap);

    // when paging is implemented, it should be activated here

    // enable the heap, so that the alloc crate can be used
    let heap_location = 0x400000;
    heap::init_allocator(heap_location);

}

/// Initialize the hardware necessary to run the PC architecture
pub fn init_hardware() {
    PIC::new().init();
    // set the PIT interrupt to approximately 100Hz
    PIT::new().set_divider(11932);

    crate::hardware::pci::init();
}


