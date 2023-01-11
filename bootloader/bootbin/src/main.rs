#![no_std]
#![no_main]

mod disk;
mod elf;
mod gdt;
mod video;

use core::arch::asm;
use core::fmt::Write;

use crate::elf::{ElfHeader, SectionHeader};

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
    let mut kernel_sectors = file_size / 512;
    if file_size & 511 != 0 {
        kernel_sectors += 1;
    }
    let mut first_kernel_sector = root_data_sector;
    first_kernel_sector += sectors_per_cluster * first_cluster;
    first_kernel_sector -= sectors_per_cluster * 2;
    write!(video::VideoWriter, "Kernel at sector {:#X}, {:#X} sectors long\r\n", first_kernel_sector, kernel_sectors);
    write!(video::VideoWriter, "Disk No: {:#x}\r\n", disk_number);
    // only memory below 1MB is available to BIOS, so we need to first copy to
    // a lowmem buffer, and then copy that to higher memory when it's ready
    // TODO: account for a kernel that is larger than 64KB
    if kernel_sectors > 128 {
        panic!("Cannot copy more than 64KB at a time");
    }
    disk::read_sectors(disk_number, first_kernel_sector, 0x800, 0, kernel_sectors as u16); 

    // Read the ELF header after the first chunk is copied to memory
    // If we do this after it's been copied to high memory, the compiler will
    // do weird optimizations assuming that addresses can't be higher than 1MB
    // and everything will break.
    let sections_end = unsafe {
        let elf_root_ptr = 0x8000 as *const ElfHeader;
        let elf_root = &(*elf_root_ptr);
        let mut section_header_addr = 0x8000 + elf_root.section_header_location;
        let section_header_size = elf_root.section_header_entry_size as u32;
        let mut section_header_count = elf_root.section_header_entry_count;

        let mut sections_end: u32 = 0;
        while section_header_count > 0 {
            let section_header_ptr = section_header_addr as *const SectionHeader;
            let header = &(*section_header_ptr);
            let section_load_at = header.section_address;
            let section_end_at = section_load_at + header.section_size;
            if section_end_at > sections_end {
                sections_end = section_end_at;
            }
            section_header_count -= 1;
            section_header_addr += section_header_size;
        }
        sections_end
    };

    let entry_addr = unsafe { *(0x8018 as *const u32) };

    // copy from low memory buffer to 1MB mark
    unsafe {
        let src = 0x8000 as *const u8;
        let dst = 0x100000 as *mut u8;
        core::ptr::copy_nonoverlapping(src, dst, kernel_sectors as usize * 512);
    }

    // enter protected mode, jump to 32-bit section of bootbin
    unsafe {
        asm!(
            "mov esp, ecx",
            "sub esp, 4",
            "push eax",
            "mov eax, cr0",
            "or eax, 1",
            "mov cr0, eax",
            in("ecx") sections_end,
            in("eax") entry_addr,
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
            "pop eax",
            "call eax",
        );
    }

    loop {}
}

#[panic_handler]
pub fn panic(info: &core::panic::PanicInfo) -> ! {
    video::print_string("PANIC");

    loop {}
}
