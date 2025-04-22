use crate::hardware::{pic::PIC, pit::PIT};
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::heap;
use crate::memory::physical::bios::BIOS_MEMORY_MAP_LOCATION;
use crate::memory::physical::init_allocator;
use crate::memory::physical::range::FrameRange;
use crate::task::stack::get_kernel_stack_virtual_offset;
use core::arch::asm;

#[allow(improper_ctypes)]
extern "C" {
    #[link_name = "__kernel_start"]
    static mut label_kernel_start: ();
    #[link_name = "__bss_start"]
    static mut label_bss_start: u8;
    #[link_name = "__bss_end"]
    static label_bss_end: u8;
    #[link_name = "__kernel_end"]
    static label_kernel_end: ();
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
    let kernel_start_addr = &raw const label_kernel_start as u32;
    let kernel_end_addr = &label_kernel_end as *const () as u32;
    let kernel_range = FrameRange::new(
        PhysicalAddress::new(kernel_start_addr),
        kernel_end_addr - kernel_start_addr,
    );
    init_allocator(
        PhysicalAddress::new(kernel_end_addr),
        BIOS_MEMORY_MAP_LOCATION,
        kernel_range,
    );
    crate::kprint!("KERNEL RANGE: {:?}\n", kernel_range);

    let allocator_end = kernel_end_addr + crate::memory::physical::get_allocator_size() as u32;
    crate::kprint!("ALLOC END: {:X}\n", allocator_end);
    let heap_start = VirtualAddress::new(0xc0000000 + allocator_end);

    // activate paging and virtual memory
    let initial_mapped_range =
        VirtualAddress::new(kernel_start_addr)..VirtualAddress::new(allocator_end);
    let initial_pagedir =
        crate::memory::virt::create_initial_pagedir(initial_mapped_range, BIOS_MEMORY_MAP_LOCATION);
    initial_pagedir.make_active();
    crate::memory::virt::enable_paging();

    // relocate $esp to the virtual location of the initial kernel stack
    let stack_offset = get_kernel_stack_virtual_offset();
    asm!(
        "add esp, {offset}",
        offset = in(reg) stack_offset,
    );
    crate::memory::physical::with_allocator(|alloc| alloc.move_to_highmem());

    // enable the heap, so that the alloc crate can be used
    heap::init_allocator(heap_start);
}

/// Initialize the hardware necessary to run the PC architecture
pub fn init_hardware() {
    PIC::new().init();
    // set the PIT interrupt to approximately 100Hz
    PIT::new().set_divider(11932);

    crate::hardware::pci::get_bus_devices();

    crate::time::system::initialize_time_from_rtc();
}

/// Populate the DEV: FS with drivers for the devices detected on this PC
pub fn init_device_drivers() {
    crate::hardware::com::driver::install();
}
