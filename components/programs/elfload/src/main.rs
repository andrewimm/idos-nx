#![no_std]
#![no_main]
#![feature(lang_items)]

extern crate idos_api;

mod elf;
mod panic;

use core::arch::{asm, global_asm};
use elf::{ElfHeader, ProgramHeader, ELF_MAGIC, SEGMENT_FLAG_WRITE, SEGMENT_TYPE_LOAD};
use idos_api::{
    io::sync::{close_sync, open_sync, read_sync},
    syscall::{
        io::create_file_handle,
        memory::{map_file, map_memory, MMAP_SHARED},
    },
};

const LOAD_INFO_MAGIC: u32 = 0x4C4F4144;

/// The stack top address — must match what the kernel sets up
const STACK_TOP: u32 = 0xc000_0000;

/// Layout of the load info page header, matching kernel/src/exec.rs
#[repr(C)]
struct LoadInfoHeader {
    magic: u32,
    exec_path_offset: u32,
    exec_path_len: u32,
    argc: u32,
    argv_offset: u32,
    argv_total_len: u32,
}

// Maximum number of program headers we support
const MAX_PROGRAM_HEADERS: usize = 16;

global_asm!(
    r#"
.global _start

_start:
    push ebx
    call loader_start
"#
);

#[no_mangle]
pub extern "C" fn loader_start(load_info_addr: u32) -> ! {
    // Read the load info page
    let header = unsafe { &*(load_info_addr as *const LoadInfoHeader) };

    if header.magic != LOAD_INFO_MAGIC {
        idos_api::syscall::exec::terminate(0xff);
    }

    // Extract the executable path
    let exec_path = unsafe {
        let path_ptr = (load_info_addr + header.exec_path_offset) as *const u8;
        core::slice::from_raw_parts(path_ptr, header.exec_path_len as usize)
    };
    let exec_path_str = unsafe { core::str::from_utf8_unchecked(exec_path) };

    // Open the executable file
    let file_handle = create_file_handle();
    if open_sync(file_handle, exec_path_str).is_err() {
        idos_api::syscall::exec::terminate(0xfe);
    }

    // Read the ELF header
    let mut elf_header = ElfHeader::default();
    let elf_header_bytes = unsafe {
        core::slice::from_raw_parts_mut(
            &mut elf_header as *mut ElfHeader as *mut u8,
            core::mem::size_of::<ElfHeader>(),
        )
    };
    if read_sync(file_handle, elf_header_bytes, 0).is_err() {
        idos_api::syscall::exec::terminate(0xfd);
    }

    if elf_header.magic != ELF_MAGIC {
        idos_api::syscall::exec::terminate(0xfc);
    }

    let entry_point = elf_header.entry_point;
    let ph_count = elf_header.program_header_count as usize;
    let ph_size = elf_header.program_header_size as u32;

    if ph_count > MAX_PROGRAM_HEADERS {
        idos_api::syscall::exec::terminate(0xfb);
    }

    // Read and process program headers
    let mut ph_offset = elf_header.program_header_offset;
    for _ in 0..ph_count {
        let mut ph = ProgramHeader::default();
        let ph_bytes = unsafe {
            core::slice::from_raw_parts_mut(
                &mut ph as *mut ProgramHeader as *mut u8,
                core::mem::size_of::<ProgramHeader>(),
            )
        };
        if read_sync(file_handle, ph_bytes, ph_offset).is_err() {
            idos_api::syscall::exec::terminate(0xfa);
        }

        if ph.segment_type == SEGMENT_TYPE_LOAD {
            map_load_segment(exec_path_str, &ph);
        }

        ph_offset += ph_size;
    }

    let _ = close_sync(file_handle);

    // Set up the stack with argc/argv and jump to the entry point
    setup_stack_and_jump(load_info_addr, header, entry_point);
}

/// Map a single PT_LOAD segment from the executable file.
fn map_load_segment(exec_path: &str, ph: &ProgramHeader) {
    let vaddr = ph.virtual_address;
    let file_offset = ph.offset;
    let file_size = ph.file_size;
    let memory_size = ph.memory_size;
    let writable = ph.flags & SEGMENT_FLAG_WRITE != 0;

    // Page-align everything
    let vaddr_aligned = vaddr & 0xfffff000;
    let file_offset_aligned = file_offset & 0xfffff000;
    let end_addr = (vaddr + memory_size + 0xfff) & 0xfffff000;
    let file_end_addr = (vaddr + file_size + 0xfff) & 0xfffff000;
    let mapped_size = file_end_addr - vaddr_aligned;

    // Map the file-backed portion
    if mapped_size > 0 {
        let flags = if writable { 0 } else { MMAP_SHARED };
        let _ = map_file(
            Some(vaddr_aligned),
            mapped_size,
            exec_path,
            file_offset_aligned,
            flags,
        );
    }

    // If memory_size > file_size, we need extra zero pages for BSS
    let bss_start = file_end_addr;
    if end_addr > bss_start {
        let bss_size = end_addr - bss_start;
        let _ = map_memory(Some(bss_start), bss_size, None);
    }
}

/// Set up argc/argv on the stack and jump to the executable's entry point.
///
/// The load info page contains argv data as null-terminated strings
/// concatenated together (from ExecArgs). We need to convert this to the
/// C-style layout that the SDK's _start expects:
///
///   [esp]     = argc
///   [esp + 4] = argv[0] pointer
///   [esp + 8] = argv[1] pointer
///   ...
///   (string data higher on stack)
fn setup_stack_and_jump(
    load_info_addr: u32,
    header: &LoadInfoHeader,
    entry_point: u32,
) -> ! {
    let argc = header.argc;
    let argv_data_ptr = (load_info_addr + header.argv_offset) as *const u8;
    let argv_total_len = header.argv_total_len as usize;

    // We'll build the stack at STACK_TOP, growing downward.
    // Layout (top to bottom):
    //   - raw string data (copied from load info argv)
    //   - argv pointer array
    //   - argc

    let stack_page = (STACK_TOP - 0x1000) as *mut u8;

    // Copy raw argv string data to the top of the stack page
    // Align the start down to 4 bytes
    let mut strings_start = 0x1000 - argv_total_len;
    strings_start &= !3;

    if argv_total_len > 0 {
        unsafe {
            let dest = stack_page.add(strings_start);
            core::ptr::copy_nonoverlapping(argv_data_ptr, dest, argv_total_len);
        }
    }

    // The string data lives at virtual address (STACK_TOP - 0x1000 + strings_start)
    let strings_vaddr = STACK_TOP - 0x1000 + strings_start as u32;

    // Build argv pointer array below the strings
    let argv_array_size = argc as usize * 4;
    let argv_array_start = strings_start - argv_array_size;
    let argv_array_ptr = unsafe { stack_page.add(argv_array_start) as *mut u32 };

    // Walk through the null-terminated strings to build pointers
    let mut string_offset: u32 = 0;
    for i in 0..argc as usize {
        unsafe {
            *argv_array_ptr.add(i) = strings_vaddr + string_offset;
        }
        // Find the next null terminator
        let mut j = string_offset as usize;
        while j < argv_total_len {
            let byte = unsafe { *argv_data_ptr.add(j) };
            if byte == 0 {
                break;
            }
            j += 1;
        }
        string_offset = (j + 1) as u32; // skip past the null
    }

    // Place argc below the argv array
    let argc_offset = argv_array_start - 4;
    unsafe {
        *(stack_page.add(argc_offset) as *mut u32) = argc;
    }

    // ESP points to argc
    let new_esp = STACK_TOP - 0x1000 + argc_offset as u32;

    unsafe {
        asm!(
            "mov esp, {esp}",
            "jmp {entry}",
            esp = in(reg) new_esp,
            entry = in(reg) entry_point,
            options(noreturn),
        );
    }
}

#[lang = "eh_personality"]
pub extern "C" fn eh_personality() {}
