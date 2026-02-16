#![no_std]
#![no_main]

mod disk;
mod elf;
mod gdt;
mod video;

use core::fmt::Write;
use core::{any::Any, arch::asm};

use crate::elf::{ElfHeader, SectionHeader};

static mut KERNEL_MEMORY_END: u32 = 0;
static mut KERNEL_ENTRY_LOCATION: u32 = 0;

/// Entry point for BOOTBIN, the IDOS bootloader
/// The bootloader is launched by the MBR code, and is responsible for running
/// any code that needs BIOS interrupts. Then, it loads the kernel from disk
/// into memory. Finally, it enters protected mode and jumps into kernel execution.
/// It's assumed that the MBR has established a valid stack, likely below the
/// area where MBR code has been loaded.
#[no_mangle]
#[link_section = ".entry"]
pub unsafe extern "C" fn _start(fat_metadata: *const disk::FatMetadata) -> ! {
    // Enable A20 line to make all memory accessible
    // The "Fast A20" gate can be tested and enabled via port 0x92 on the PS/2 controller
    asm!(
        "push ax",
        "in al, 0x92",
        "test al, 2",
        "jnz 2f",
        "or al, 2",
        "and al, 0xfe",
        "out 0x92, al",
        "2:",
        "pop ax",
    );

    // Print something for debugging, to indicate BOOTBIN has begun
    video::print_string("= IDOS BOOTBIN =\r\n");

    // disable interrupts, this is gonna get messy
    asm!("cli", options(nostack, nomem, preserves_flags),);

    // We have a default GDT table with entries for code and data
    let gdt_pointer = gdt::GdtPointer::new(&gdt::INITIAL_GDT);
    // enter unreal mode by loading the GDT, entering protected mode,
    // caching all of the segment registers, and then jumping back to real mode
    gdt_pointer.load();
    asm!(
        "push ds",      // save DS for when we jump back to real mode
        "push ss",      // same with SS. These will ensure stack and data are still reachable
        "push ax",      // push registers we're
        "push bx",      //   going to trash
        "mov eax, cr0", // over the next 3 lines,
        "or eax, 1",    //   toggle the protected mode bit
        "mov cr0, eax", //   and store it back in CR0
        "jmp 2f",       // by jumping, protected mode is actually activated
        "2:",
        "mov bx, 0x10", // use the data segment we set up in the GDT
        "mov ds, bx",   //   and store it in both DS
        "mov ss, bx",   //   and SS, giving these access to all of memory
        "and al, 0xfe", // ax still stores the previous value of CR0
        "mov cr0, eax", // clear the protected mode bit and store it back in CR0
        "jmp 3f",       // the jump forces back to real mode
        "3:",
        "pop bx", // restore trashed registers
        "pop ax",
        "pop ss", // as well as the data segments
        "pop ds",
    );

    // Map memory using BIOS interrupts
    // This is one of the benefits of staying in real mode a bit longer.
    // We still have access to BIOS code without having to jump into a 8086 VM
    asm!(
        "push esi", // Because we know we're on a 32-bit processor
        "push edi", // we can use the full 32-bit registers with a special prefix
        "push eax", // Temporarily stash the registers we're going to use
        "push ecx",
        "push edx",
        "push ebx",
        "xor esi, esi", // Begin setting up the arguments for the memory map call
        "xor ebx, ebx",
        "mov edi, 0x1004", // ES:DI points to the location in memory where it should be stored
        "2:",
        "mov edx, 0x534d4150", // EDX contains a magic number: "SMAP"
        "mov eax, 0xe820",     // Select the memmory map function
        "mov ecx, 24",
        "int 0x15",   // int 0x15, with EAX=0xe820, calls the memory map function
        "jc 3f",      // carry flag will set when done reading
        "cmp ebx, 0", // or bx will be zero
        "je 3f",
        "cmp eax, edx", // on success, EAX will also equal "SMAP"
        "jne 3f",       // end if it wasn't successful
        "add di, 24",   // each entry is 24 bytes long, so increment DI to the next write location
        "inc esi",
        "cmp esi, 170", // arbitrarily cap at 170 runs, for a limit of 0x1000 bytes
        "jb 2b",
        "3:",
        "mov [0x1000], esi", // we saved the first 4 bytes for ESI, which contains the number of entries
        "pop ebx",           // and restore all the registers that were used
        "pop edx",
        "pop ecx",
        "pop eax",
        "pop edi",
        "pop esi",
    );

    // Find the kernel file, and load it into memory
    // Some of the necessary numbers for interacting with the boot drive were
    // stored by MBR in the fat_metadata struct
    let disk_number: u8 = unsafe { (*fat_metadata).disk_number };
    let root_data_sector: u16 = unsafe { (*fat_metadata).root_cluster_sector };
    let sectors_per_cluster: u16 = unsafe { (*fat_metadata).sectors_per_cluster };
    let (first_cluster, file_size) = match disk::find_root_dir_file("KERNEL  BIN") {
        Some(pair) => pair,
        None => {
            video::print_string("Kernel not found!");
            loop {
                // Don't spin the CPU needlessly
                asm!("cli; hlt");
            }
        }
    };
    video::print_string("Kernel found, loading into memory.\r\n");
    let kernel_sectors = bytes_to_sectors(file_size, 512);

    let mut first_kernel_sector = root_data_sector;
    first_kernel_sector += sectors_per_cluster * first_cluster;
    first_kernel_sector -= sectors_per_cluster * 2;
    write!(
        video::VideoWriter,
        "Kernel at sector {:#X}, {:#X} sectors long\r\n",
        first_kernel_sector,
        kernel_sectors
    )
    .unwrap();
    write!(video::VideoWriter, "Disk No: {:#x}\r\n", disk_number).unwrap();
    // only memory below 1MB is available to BIOS, so we need to first copy to
    // a lowmem buffer, and then copy that to higher memory when it's ready.
    // The segmented memory model also makes it easiest to just copy in 64KiB
    // chunks.
    let max_sectors_per_copy = 128;
    let mut kernel_copy_sector = first_kernel_sector;
    let mut remaining_sectors = kernel_sectors as u16;
    let mut total_bytes_copied = 0;
    let mut section_header_location = 0;
    let mut section_header_entry_size = 0;
    let mut section_header_entry_count = 0;

    loop {
        let sector_copy_count = max_sectors_per_copy.min(remaining_sectors) as u16;
        // copy up to 128 sectors to 0x8000
        disk::read_sectors(disk_number, kernel_copy_sector, 0x800, 0, sector_copy_count);
        let copy_size = sector_copy_count as u32 * 512;

        write!(
            video::VideoWriter,
            "Copy {:#X} bytes at 0x8000\r\n",
            copy_size
        )
        .unwrap();

        if kernel_copy_sector == first_kernel_sector {
            // First sector has the ELF header
            let elf_root_ptr = 0x8000 as *const ElfHeader;
            let elf_root = &(*elf_root_ptr);
            section_header_location = elf_root.section_header_location;
            KERNEL_ENTRY_LOCATION = core::ptr::read_volatile(0x8018 as *const u32);

            section_header_entry_size = elf_root.section_header_entry_size as u32;
            section_header_entry_count = elf_root.section_header_entry_count;
        }

        {
            let src = 0x8000 as *const u8;
            let dst = (0x100000 + total_bytes_copied) as *mut u8;
            core::ptr::copy_nonoverlapping(src, dst, copy_size as usize);
        }

        total_bytes_copied += copy_size;

        if remaining_sectors <= max_sectors_per_copy {
            break;
        }

        remaining_sectors -= sector_copy_count;
        kernel_copy_sector += sector_copy_count;
    }

    // Now that the entire kernel has been copied to high memory, walk the
    // section headers directly from the copy at 0x100000+. This avoids
    // boundary issues with the 64KiB copy buffer and ensures NOBITS zeroing
    // happens after all data has been written.
    {
        let mut sections_end = 0u32;
        let mut section_header_addr = (0x100000 + section_header_location) as *const SectionHeader;
        for _ in 0..section_header_entry_count {
            let header = &*section_header_addr;
            let section_load_at = header.section_address;
            let section_end_at = section_load_at + header.section_size;
            if section_end_at > sections_end {
                sections_end = section_end_at;
            }

            // if the header type is NOBITS, we should zero it now since the
            // executable assumes it is zeroed out
            if header.header_type == 0x8 {
                assert!(header.section_size & 3 == 0);
                let zero_start = (0x100000 + header.section_offset) as *mut u32;
                let zero_size = header.section_size as isize;
                for offset in 0..(zero_size / 4) {
                    core::ptr::write_volatile(zero_start.offset(offset), 0);
                }

                write!(
                    video::VideoWriter,
                    "ZERO from {:#X}, len {:#X}\r\n",
                    zero_start as u32,
                    zero_size
                );
            }

            section_header_addr = ((section_header_addr as u32) + section_header_entry_size) as *const SectionHeader;
        }

        KERNEL_MEMORY_END = sections_end;
        KERNEL_MEMORY_END -= 0xc0000000;
    }

    write!(
        video::VideoWriter,
        "Total bytes copied: {}\r\n",
        total_bytes_copied
    )
    .unwrap();

    // Now that the kernel has been copied through the buffer to high memory,
    // we should be able to access free memory found at 0x8000.
    //
    // Since the kernel is designed to live at 0xc0000000, we need to enable
    // paging and simply map the first 4MiB to that area.
    //
    // Use the 4KiB frame at 0x8000 as the zero-th page table, and the
    // frame at 0x9000 as the page directory
    let page_table_start = 0x8000 as *mut u32;
    let page_table_slice = core::slice::from_raw_parts_mut(page_table_start, 0x400);
    let flags: u32 = 1; // present; no other flags needed
    for i in 0..page_table_slice.len() {
        let addr = (i as u32) << 12;
        page_table_slice[i] = addr | flags;
    }

    let page_dir_start = 0x9000 as *mut u32;
    let page_dir_slice = core::slice::from_raw_parts_mut(page_dir_start, 0x400);
    for i in 0..page_dir_slice.len() {
        page_dir_slice[i] = 0;
    }
    page_dir_slice[0] = (8 << 12) | 1;
    // also map the first 4 MiB to 0xc0000000, since that's where the kernel
    // thinks it lives
    page_dir_slice[0x300] = (8 << 12) | 1;

    // load the page directory by writing 0x9000 to cr3
    asm!("push eax", "mov eax, 0x9000", "mov cr3, eax", "pop eax",);

    write!(
        video::VideoWriter,
        "Enter kernel at {:#010X}, esp {:#010X}\r\n",
        KERNEL_ENTRY_LOCATION,
        KERNEL_MEMORY_END,
    )
    .unwrap();

    // enter protected mode, jump to 32-bit section of bootbin
    asm!(
        // the kernel ELF contains 0x1000 bytes at the end that can be used
        "mov esp, ecx",
        "push eax",
        "mov eax, cr0",
        "or eax, 0x00000001",
        "mov cr0, eax",
        in("ecx") KERNEL_MEMORY_END,
        in("eax") KERNEL_ENTRY_LOCATION,
    );
    asm!(
        "ljmp $0x08, $2f",
        "2:",
        options(att_syntax), // long jump in LLVM intel syntax is broken
    );
    asm!(
        ".code32",
        "mov ax, 0x10",
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",
        "mov ss, ax",
        "mov eax, cr0",
        "or eax, 0x80000000",
        "mov cr0, eax",
        "pop eax",
        "add esp, 0xc0000000",
        "call eax",
    );

    panic!("Kernel returned to BOOTBIN");
}

fn bytes_to_sectors(bytes: u32, sector_size: u32) -> u32 {
    let sectors = bytes / sector_size;
    let addl = if bytes & (sector_size - 1) == 0 { 0 } else { 1 };
    sectors + addl
}

#[panic_handler]
pub fn panic(info: &core::panic::PanicInfo) -> ! {
    video::print_string("PANIC: ");
    write!(video::VideoWriter, "{}", info);

    loop {
        unsafe {
            asm!("cli; hlt");
        }
    }
}
