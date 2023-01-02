
/// Zero out the .bss section. Code may assume this area starts as zeroes.
pub unsafe fn zero_bss() {
}

/// Initialize the GDT, IDT
pub unsafe fn init_cpu_tables() {
}

/// Initialize system memory, enabling virtual memory and paging.
/// Once virtual memory has been enabled, all references to kernel addresses
/// need to be or-ed with 0xc0000000 so that they can correctly point to the
/// kernel in all tasks.
pub unsafe fn init_memory() {
}


