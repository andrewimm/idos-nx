use core::arch::asm;
use crate::filesystem::install_device_driver;
use crate::hardware::{pic::PIC, pit::PIT};
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::heap;
use crate::memory::physical::init_allocator;
use crate::memory::physical::range::FrameRange;
use crate::task::stack::get_kernel_stack_virtual_offset;

#[allow(improper_ctypes)]
extern {
    #[link_name = "__kernel_start"]
    static mut label_kernel_start: ();
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
    crate::arch::gdt::init_tss();
    let gdt = &crate::arch::gdt::GDT;
    let gdt_descriptor = &mut crate::arch::gdt::GDTR;
    gdt_descriptor.point_to(gdt);
    gdt_descriptor.load();
    crate::arch::gdt::ltr(0x28);

    crate::interrupts::idt::init_idt();
}

/// Initialize system memory, enabling virtual memory and paging.
/// Once virtual memory has been enabled, all references to kernel addresses
/// need to be or-ed with 0xc0000000 so that they can correctly point to the
/// kernel in all tasks.
pub unsafe fn init_memory() {
    let kernel_start_addr = &label_kernel_start as *const () as u32;
    let kernel_end_addr = &label_kernel_end as *const () as u32;
    let kernel_range = FrameRange::new(
        PhysicalAddress::new(kernel_start_addr),
        kernel_end_addr - kernel_start_addr,
    );
    let bios_memmap = PhysicalAddress::new(0x1000);
    init_allocator(PhysicalAddress::new(kernel_end_addr), bios_memmap, kernel_range);
    crate::kprint!("KERNEL RANGE: {:?}\n", kernel_range);

    let allocator_end = kernel_end_addr + crate::memory::physical::get_allocator_size() as u32;
    crate::kprint!("ALLOC END: {:X}\n", allocator_end);
    let heap_start = VirtualAddress::new(0xc0000000 + allocator_end);

    // activate paging and virtual memory
    let initial_pagedir = crate::memory::virt::create_initial_pagedir();
    initial_pagedir.make_active();
    crate::memory::virt::enable_paging();
    
    // relocate $esp to the virtual location of the initial kernel stack
    let stack_offset = get_kernel_stack_virtual_offset();
    asm!(
        "add esp, {offset}",
        offset = in(reg) stack_offset,
    );
    crate::kprint!("STACK RELOCATED\n");

    // enable the heap, so that the alloc crate can be used
    heap::init_allocator(heap_start);
}

/// Initialize the hardware necessary to run the PC architecture
pub fn init_hardware() {
    PIC::new().init();
    // set the PIT interrupt to approximately 100Hz
    PIT::new().set_divider(11932);

    crate::hardware::pci::init();

    crate::hardware::ps2::PS2Controller::new().init();
}

/// Populate the DEV: FS with drivers for the devices detected on this PC
pub fn init_device_drivers() {
    crate::io::com::dev::install_drivers();
}

