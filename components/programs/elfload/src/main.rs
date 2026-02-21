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
    if open_sync(file_handle, exec_path_str, 0).is_err() {
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

    // If memory_size > file_size, we need to zero-fill BSS.
    // The last file-backed page may contain a partial page where file data
    // ends partway through. The tail of that page (BSS portion) must be
    // explicitly zeroed, since map_file loads the full page from the file.
    if memory_size > file_size {
        let bss_vaddr = vaddr + file_size;
        let bss_page_offset = bss_vaddr & 0xfff;
        if bss_page_offset != 0 {
            // Zero from bss_vaddr to end of its page
            let zero_len = 0x1000 - bss_page_offset;
            unsafe {
                core::ptr::write_bytes(bss_vaddr as *mut u8, 0, zero_len as usize);
            }
        }

        // Map additional whole pages for remaining BSS
        let bss_start = file_end_addr;
        if end_addr > bss_start {
            let bss_size = end_addr - bss_start;
            let _ = map_memory(Some(bss_start), bss_size, None);
        }
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

    // We build argc/argv data at the top of the stack page, then set ESP
    // below it and jump to the entry point.
    //
    // IMPORTANT: The elfload is currently using this same stack page. Writing
    // near the top of the page will clobber the elfload's stack frame. To
    // handle this safely, we call a helper function that first relocates ESP
    // to a safe location, then writes the argv data and jumps.
    write_argv_and_jump(
        argc,
        argv_data_ptr,
        argv_total_len,
        entry_point,
    )
}

/// First relocate ESP away from the top of the stack page, then write
/// argc/argv data there safely before jumping to the entry point.
///
/// We use inline asm to move ESP to a safe location (middle of the second
/// stack page), then call the actual writer. This ensures no compiler-generated
/// stack frame overlaps with the argv data area at the top of the stack.
fn write_argv_and_jump(
    argc: u32,
    argv_data_ptr: *const u8,
    argv_total_len: usize,
    entry_point: u32,
) -> ! {
    unsafe {
        asm!(
            // Move ESP to a safe location in the second stack page
            "mov esp, {safe}",
            // Push args for the writer (cdecl calling convention)
            "push {entry}",
            "push {len}",
            "push {ptr}",
            "push {argc}",
            "call {func}",
            safe = in(reg) (STACK_TOP - 0x1800u32),
            argc = in(reg) argc,
            ptr = in(reg) argv_data_ptr,
            len = in(reg) argv_total_len,
            entry = in(reg) entry_point,
            func = sym do_write_argv_and_jump,
            options(noreturn),
        );
    }
}

/// Actually write argv data and jump. Called after ESP has been relocated
/// to a safe location away from the top of the stack page.
#[inline(never)]
unsafe extern "C" fn do_write_argv_and_jump(
    argc: u32,
    argv_data_ptr: *const u8,
    argv_total_len: usize,
    entry_point: u32,
) -> ! {
    let stack_page = (STACK_TOP - 0x1000) as *mut u8;

    // Compute layout — clamp strings_start so it stays within the page
    let mut strings_start = if argv_total_len > 0 {
        (0x1000 - argv_total_len) & !3
    } else {
        0x1000 - 4 // leave room even when there are no strings
    };
    let strings_vaddr = STACK_TOP - 0x1000 + strings_start as u32;
    let argv_array_size = (argc as usize + 1) * 4;
    let argv_array_start = strings_start - argv_array_size;
    let argc_offset = argv_array_start - 4;
    let new_esp = STACK_TOP - 0x1000 + argc_offset as u32;

    // Write string data
    if argv_total_len > 0 {
        let dest = stack_page.add(strings_start);
        core::ptr::copy_nonoverlapping(argv_data_ptr, dest, argv_total_len);
    }

    // Write argv pointer array
    let argv_array_ptr = stack_page.add(argv_array_start) as *mut u32;
    let mut string_offset: u32 = 0;
    for i in 0..argc as usize {
        *argv_array_ptr.add(i) = strings_vaddr + string_offset;
        let mut j = string_offset as usize;
        while j < argv_total_len {
            if *argv_data_ptr.add(j) == 0 {
                break;
            }
            j += 1;
        }
        string_offset = (j + 1) as u32;
    }
    // argv[argc] = NULL
    *argv_array_ptr.add(argc as usize) = 0;

    // Write argc
    *(stack_page.add(argc_offset) as *mut u32) = argc;

    // Set ESP and jump
    asm!(
        "mov esp, {esp}",
        "jmp {entry}",
        esp = in(reg) new_esp,
        entry = in(reg) entry_point,
        options(noreturn),
    );
}

#[lang = "eh_personality"]
pub extern "C" fn eh_personality() {}
