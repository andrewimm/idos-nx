//! This simple MBR bootloader performs basic initialization of the CPU before
//! attempting to load a stage-2 bootloader, named BOOT.BIN, from the disk.
//! BOOT.BIN needs to be the first file entry in the FAT root directory for
//! this to work as expected.

#![no_std]
#![no_main]
#![feature(core_intrinsics)]
#![feature(naked_functions)]

mod fat;

use core::arch::{asm, global_asm};

global_asm!(include_str!("boot.s"));

#[no_mangle]
pub extern "C" fn mbr_start(disk_number: u16) -> ! {
    // initialize the FAT attributes from the MBR
    fat::init_fat(disk_number);
    // read the root dir sector to disk, to locate BOOT.BIN
    let root_dir_dest: u16 = 0x7e00;
    let root_dir_sector = unsafe { fat::FAT_DATA.root_dir_sector };
    fat::read_sectors(root_dir_sector, root_dir_dest, 1);
    // check the name of the first file
    let expected = "BOOT    BIN".as_bytes();
    let first_dir_entry = unsafe { 0x7e00 as *const u8 };
    for i in 0..11 {
        unsafe {
            let expected_char = *expected.get_unchecked(i);
            let found_char = *first_dir_entry.offset(i as isize);
            if expected_char != found_char {
                print_string("BOOT.BIN not found!");
                loop {}
            }
        }
    }

    print_string("Boot\r\n");
    // first file is BOOT.BIN
    // read all sectors of the file to 0x1000
    let boot_bin_size_ptr: *const u16 = 0x7e1d as *const u16;
    let mut boot_bin_sectors = unsafe { *boot_bin_size_ptr } >> 1;
    boot_bin_sectors += 1;

    let first_cluster_sector = unsafe { fat::FAT_DATA.root_cluster_sector };

    fat::read_sectors(first_cluster_sector, 0x1000, boot_bin_sectors);

    let boot_bin_start: extern "C" fn() = unsafe {
        core::mem::transmute(0x1000 as *const ())
    };

    boot_bin_start();

    loop {}
}

#[no_mangle]
pub extern "C" fn print_string(s: &'static str) {
    let slice = s.as_bytes();
    let ax = &slice[0] as *const u8;
    let cx = slice.len() + 1;
    unsafe {
        asm!(
            "push si",
            "mov si, ax",
            "mov ax, 0x0e00",
            "2:",
            "dec cx",
            "jz 3f",
            "lodsb",
            "int 0x10",
            "jmp 2b",
            "3:",
            "pop si",
            in("ax") ax,
            in("cx") cx,
        );
    }
}

#[no_mangle]
pub extern "C" fn print_char(ch: u8) {
    let ax = (ch as u16) | 0x0e00;
    let bx = 0;
    unsafe {
        asm!(
            "int 0x10",
            in("bx") bx,
            in("ax") ax,
        );
    }
}

#[panic_handler]
pub fn panic(_info: &core::panic::PanicInfo) -> ! {
    print_char(b'X');
    loop {}
}
