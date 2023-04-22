#![no_std]
#![no_main]

mod disk;
mod elf;
mod gdt;
mod video;

use core::arch::asm;
use core::fmt::Write;

use crate::elf::{ElfHeader, SectionHeader};

static mut KERNEL_MEMORY_END: u32 = 0;
static mut KERNEL_ENTRY_LOCATION: u32 = 0;

#[no_mangle]
#[link_section = ".entry"]
pub extern "C" fn _start(fat_metadata: *const disk::FatMetadata) -> ! {
    // enable A20 line
    unsafe {
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
    }

    video::print_string("= IDOS BOOTBIN =\r\n");    

    // disable interrupts, this is gonna get messy
    unsafe {
        asm!(
            "cli",
            options(nostack, nomem, preserves_flags),
        );
    }

    let gdt_pointer = gdt::GdtPointer::new(&gdt::INITIAL_GDT);
    // enter unreal mode
    gdt_pointer.load();
    unsafe {
        asm!(
            "push ds",
            "push ss",
            "push ax",
            "push bx",
            "mov eax, cr0",
            "or eax, 1",
            "mov cr0, eax",
            "jmp 2f",

            "2:",
            "mov bx, 0x10",
            "mov ds, bx",
            "mov ss, bx",
            "and al, 0xfe",
            "mov cr0, eax",
            "jmp 3f",

            "3:",
            "pop bx",
            "pop ax",
            "pop ss",
            "pop ds",
        );
    }

    // map memory using BIOS interrupts
    
    unsafe {
        asm!(
            "push esi",
            "push edi",
            "push eax",
            "push ecx",
            "push edx",
            "push ebx",

            "xor esi, esi",
            "xor ebx, ebx",
            "mov edi, 0x1004",

            "2:",
            "mov edx, 0x534d4150",
            "mov eax, 0xe820",
            "mov ecx, 24",
            "int 0x15",
            "jc 3f",
            "cmp ebx, 0",
            "je 3f",
            "cmp eax, edx",
            "jne 3f",

            "add di, 24",
            "inc esi",
            // arbitrarily cap at 170, for a limit of 0x1000 bytes
            "cmp esi, 170",
            "jb 2b",

            "3:",
            "mov [0x1000], esi",

            "pop ebx",
            "pop edx",
            "pop ecx",
            "pop eax",
            "pop edi",
            "pop esi",
        );
    }

    // find the kernel file, and load it into memory
    let disk_number: u8 = unsafe { (*fat_metadata).disk_number };
    let root_data_sector: u16 = unsafe { (*fat_metadata).root_cluster_sector };
    let sectors_per_cluster: u16 = unsafe { (*fat_metadata).sectors_per_cluster };
    let (first_cluster, file_size) = match disk::find_root_dir_file("KERNEL  BIN") {
        Some(pair) => pair,
        None => {
            video::print_string("Kernel not found!");
            loop  {}
        },
    };
    video::print_string("Kernel found, loading into memory.\r\n");
    let kernel_sectors = bytes_to_sectors(file_size, 512);

    let mut first_kernel_sector = root_data_sector;
    first_kernel_sector += sectors_per_cluster * first_cluster;
    first_kernel_sector -= sectors_per_cluster * 2;
    write!(video::VideoWriter, "Kernel at sector {:#X}, {:#X} sectors long\r\n", first_kernel_sector, kernel_sectors).unwrap();
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

    unsafe {
        loop {
            let sector_copy_count = max_sectors_per_copy.min(remaining_sectors) as u16;
            // copy up to 128 sectors to 0x8000
            disk::read_sectors(disk_number, kernel_copy_sector, 0x800, 0, sector_copy_count);
            let first_byte = total_bytes_copied as u32;
            let copy_size = sector_copy_count as u32 * 512;

            if kernel_copy_sector == first_kernel_sector {
                // First sector has the ELF header
                let elf_root_ptr = 0x8000 as *const ElfHeader;
                let elf_root = &(*elf_root_ptr);
                section_header_location = elf_root.section_header_location;
                KERNEL_ENTRY_LOCATION = core::ptr::read_volatile(0x8018 as *const u32);

                section_header_entry_size = elf_root.section_header_entry_size as u32;
                section_header_entry_count = elf_root.section_header_entry_count;
            }

            if section_header_location > first_byte && section_header_location < (first_byte + copy_size) {
                let mut sections_end = 0;
                let mut section_header_addr = (section_header_location - first_byte) + 0x8000;
                while section_header_entry_count > 0 {
                    let section_header_ptr = section_header_addr as *const SectionHeader;
                    let header = &(*section_header_ptr);
                    let section_load_at = header.section_address;
                    let section_end_at = section_load_at + header.section_size;
                    if section_end_at > sections_end {
                        sections_end = section_end_at;
                    }
                    section_header_entry_count -= 1;
                    section_header_addr += section_header_entry_size;
                }

                KERNEL_MEMORY_END = sections_end;
                KERNEL_MEMORY_END -= 0xc0000000;
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
    }

    write!(video::VideoWriter, "Total bytes copied: {}\r\n", total_bytes_copied).unwrap();

    // Now that the kernel has been copied through the buffer to high memory,
    // we should be able to access free memory found at 0x8000.
    //
    // Since the kernel is designed to live at 0xc0000000, we need to enable
    // paging and simply map the first 4MiB to that area.
    unsafe {
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
        page_dir_slice[0x300] = (8 << 12) | 1;

        asm!(
            "push eax",
            "mov eax, 0x9000",
            "mov cr3, eax",
            "pop eax",
        );
    }

    unsafe {
        write!(
            video::VideoWriter,
            "Enter kernel at {:#010X}, esp {:#010X}\r\n",
            KERNEL_ENTRY_LOCATION,
            KERNEL_MEMORY_END,
        ).unwrap();
    }

    // enter protected mode, jump to 32-bit section of bootbin
    unsafe {
        asm!(
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
    }

    loop {}
}

fn bytes_to_sectors(bytes: u32, sector_size: u32) -> u32 {
    let sectors = bytes / sector_size;
    let addl = if bytes & (sector_size - 1) == 0 {
        0
    } else {
        1
    };
    sectors + addl
}

#[panic_handler]
pub fn panic(_info: &core::panic::PanicInfo) -> ! {
    video::print_string("PANIC");

    loop {}
}
