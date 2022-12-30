#![no_std]
#![no_main]

mod gdt;
mod video;

use core::arch::asm;

#[no_mangle]
#[link_section = ".entry"]
pub extern "C" fn _start() -> ! {
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

    // enter unreal mode

    // map memory using BIOS interrupts

    // find the kernel file, and load it into memory

    // enter protected mode, jump to 32-bit section of bootbin
    unsafe {
        asm!(
            "cli",
            options(nostack, nomem, preserves_flags),
        );
    }
    let gdt_pointer = gdt::GdtPointer::new();
    gdt_pointer.load();

    // TODO: set up empty IDT, too

    unsafe {
        asm!(
            "mov eax, cr0",
            "or eax, 1",
            "mov cr0, eax",
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
            "and esp, 0xfffffffc",
            "hlt",
        );
    }

    loop {}
}

#[panic_handler]
pub fn panic(info: &core::panic::PanicInfo) -> ! {
    video::print_string("PANIC");

    loop {}
}
