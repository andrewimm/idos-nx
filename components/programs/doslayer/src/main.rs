//! In order for DOS programs to run, IDOS needs to put a DOS compatibility
//! layer in higher userspace memory. This 32-bit code runs in a loop, entering
//! a 8086 VM before returning on an interrupt or GPF fault.
//!
//! DOSLAYER is loaded by the kernel's exec_program as a userspace loader (like
//! ELFLOAD for ELF binaries). It receives a load info page via EBX containing
//! the path to the DOS executable, then maps and loads the program itself.
//! Supports both .COM files and MZ .EXE files (with relocations).

#![no_std]
#![no_main]
#![feature(lang_items)]

extern crate idos_api;

pub mod api;
pub mod panic;
use core::arch::global_asm;

use idos_api::{
    compat::VMRegisters,
    io::{
        file::FileStatus,
        sync::{close_sync, io_sync, ioctl_sync, open_sync, read_sync},
        termios::Termios,
        Handle, FILE_OP_STAT,
    },
    syscall::{io::create_file_handle, memory::map_memory},
};

const LOAD_INFO_MAGIC: u32 = 0x4C4F4144;

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

/// MZ executable header (28 bytes at the start of a DOS .EXE file)
#[repr(C, packed)]
#[derive(Default)]
struct MzHeader {
    magic: [u8; 2],           // 'M','Z' or 'Z','M'
    last_page_bytes: u16,     // bytes used in last 512-byte page
    total_pages: u16,         // total number of 512-byte pages
    relocation_count: u16,    // number of relocation entries
    header_paragraphs: u16,   // header size in 16-byte paragraphs
    min_extra_paragraphs: u16,
    max_extra_paragraphs: u16,
    initial_ss: u16,          // initial SS relative to load segment
    initial_sp: u16,          // initial SP
    checksum: u16,
    initial_ip: u16,          // initial IP
    initial_cs: u16,          // initial CS relative to load segment
    relocation_offset: u16,   // offset of relocation table in file
    overlay_number: u16,
}

/// A single MZ relocation entry: segment:offset pair
#[repr(C, packed)]
#[derive(Default)]
struct MzRelocation {
    offset: u16,
    segment: u16,
}

/// DOS segment base address (flat). Segment number is PSP_BASE / 16.
const PSP_BASE: u32 = 0x8000;
/// PSP segment number for 8086 VM registers
const PSP_SEGMENT: u32 = PSP_BASE / 16; // 0x800
/// The program image loads 0x10 paragraphs (256 bytes) past the PSP segment
const PROGRAM_SEGMENT: u32 = PSP_SEGMENT + 0x10;
/// Top of conventional memory available to DOS programs (640KB boundary)
const DOS_MEM_TOP: u32 = 0xA000_0;
/// Top of memory as a segment
const DOS_MEM_TOP_SEGMENT: u16 = (DOS_MEM_TOP / 16) as u16;

/// Program Segment Prefix — the 256-byte header DOS places before every program.
#[repr(C, packed)]
struct Psp {
    /// 0x00: INT 20h instruction (CD 20)
    int20: [u8; 2],
    /// 0x02: Top of memory segment
    mem_top_segment: u16,
    /// 0x04: Reserved
    _reserved1: u8,
    /// 0x05: Far call to DOS dispatcher (5 bytes)
    dos_far_call: [u8; 5],
    /// 0x0A: Terminate address (IP:CS)
    terminate_vector: u32,
    /// 0x0E: Ctrl-Break handler (IP:CS)
    break_vector: u32,
    /// 0x12: Critical error handler (IP:CS)
    error_vector: u32,
    /// 0x16: Parent PSP segment
    parent_psp: u16,
    /// 0x18: Job File Table (20 entries)
    jft: [u8; 20],
    /// 0x2C: Environment segment
    env_segment: u16,
    /// 0x2E: SS:SP on last INT 21h
    last_stack: u32,
    /// 0x32: JFT size
    jft_size: u16,
    /// 0x34: JFT far pointer
    jft_pointer: u32,
    /// 0x38: Previous PSP far pointer
    prev_psp: u32,
    /// 0x3C: Reserved
    _reserved2: [u8; 20],
    /// 0x50: INT 21h / RETF trampoline
    int21_retf: [u8; 3],
    /// 0x53: Reserved
    _reserved3: [u8; 45],
    /// 0x80: Command tail length
    cmdtail_len: u8,
    /// 0x81: Command tail (127 bytes, CR-terminated)
    cmdtail: [u8; 127],
}

fn setup_psp() {
    let psp = unsafe { &mut *(PSP_BASE as *mut Psp) };
    // Zero the whole thing first
    unsafe {
        core::ptr::write_bytes(PSP_BASE as *mut u8, 0, 256);
    }
    psp.int20 = [0xCD, 0x20];
    psp.mem_top_segment = DOS_MEM_TOP_SEGMENT;
    psp.int21_retf = [0xCD, 0x21, 0xCB];
    // Standard JFT: stdin=0, stdout=1, stderr=1, stdaux=2, stdprn=0xFF
    psp.jft[0] = 0x00; // stdin
    psp.jft[1] = 0x01; // stdout
    psp.jft[2] = 0x01; // stderr
    psp.jft[3] = 0x02; // stdaux
    psp.jft[4] = 0xFF; // stdprn (not open)
    for i in 5..20 {
        psp.jft[i] = 0xFF;
    }
    psp.jft_size = 20;
    // Command tail: empty
    psp.cmdtail_len = 0;
    psp.cmdtail[0] = 0x0D;
}

global_asm!(
    r#"
.global _start

_start:
    push ebx
    call dos_loader_start
"#
);

static mut TERMIOS_ORIG: Termios = Termios::default();
static STDIN: Handle = Handle::new(0);
static KBD_HANDLE: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
/// IRQ mask passed to enter_8086, built up as the DOS program sets interrupt vectors
static mut VM86_IRQ_MASK: u32 = 0;
/// Virtual interrupt flag — tracks whether the DOS program has done CLI/STI
static mut VM86_IF: bool = true;

#[no_mangle]
pub extern "C" fn dos_loader_start(load_info_addr: u32) -> ! {
    // 1. Read load info page
    let header = unsafe { &*(load_info_addr as *const LoadInfoHeader) };

    if header.magic != LOAD_INFO_MAGIC {
        idos_api::syscall::exec::terminate(0xff);
    }

    // 2. Extract executable path
    let exec_path = unsafe {
        let path_ptr = (load_info_addr + header.exec_path_offset) as *const u8;
        core::slice::from_raw_parts(path_ptr, header.exec_path_len as usize)
    };
    let exec_path_str = unsafe { core::str::from_utf8_unchecked(exec_path) };

    // 3. Open the executable and read the first 2 bytes to detect format
    let file_handle = create_file_handle();
    if open_sync(file_handle, exec_path_str, 0).is_err() {
        idos_api::syscall::exec::terminate(0xfe);
    }

    let mut magic: [u8; 2] = [0; 2];
    let _ = read_sync(file_handle, &mut magic, 0);

    let is_mz = (magic == [b'M', b'Z']) || (magic == [b'Z', b'M']);

    if is_mz {
        load_mz_exe(file_handle);
    } else {
        load_com(file_handle);
    }
}

/// Address of a default IRET stub in low memory, just past the BIOS data area.
const IRET_STUB: u32 = 0x500;
const IRET_STUB_SEGMENT: u16 = 0x0050;
const IRET_STUB_OFFSET: u16 = 0x0000;

/// Map the DOS conventional memory region: zero page (IVT/BDA) through DOS_MEM_TOP.
/// Initializes the IVT with default IRET handlers.
fn setup_dos_memory() {
    // Map the zero page for the IVT (interrupt vector table) and BIOS data area
    let _ = map_memory(Some(0), 0x1000, None);
    // Map memory from PSP_BASE up to the top of conventional DOS memory
    let dos_region_size = DOS_MEM_TOP - PSP_BASE;
    let pages = (dos_region_size + 0xfff) / 0x1000;
    let _ = map_memory(Some(PSP_BASE), pages * 0x1000, None);

    // Place an IRET instruction at the stub address
    unsafe {
        core::ptr::write_volatile(IRET_STUB as *mut u8, 0xCF); // IRET
    }

    // Point all 256 IVT entries to the IRET stub
    let ivt = 0 as *mut u16;
    for i in 0..256 {
        unsafe {
            core::ptr::write_volatile(ivt.add(i * 2), IRET_STUB_OFFSET);
            core::ptr::write_volatile(ivt.add(i * 2 + 1), IRET_STUB_SEGMENT);
        }
    }
}

/// Load a .COM file: read the entire file into memory at PSP_BASE + 0x100,
/// then enter the VM with CS:IP = PSP_SEGMENT:0x100.
fn load_com(file_handle: Handle) -> ! {
    let mut file_status = FileStatus::new();
    let _ = io_sync(
        file_handle,
        FILE_OP_STAT,
        &mut file_status as *mut FileStatus as u32,
        core::mem::size_of::<FileStatus>() as u32,
        0,
    );
    let file_size = file_status.byte_size;

    setup_dos_memory();

    setup_psp();
    read_file_into(file_handle, PSP_BASE + 0x100, file_size, 0);

    let _ = close_sync(file_handle);

    // .COM: all segment registers = PSP_SEGMENT, IP = 0x100
    compat_start(VMRegisters {
        eax: 0x00,
        ebx: 0x00,
        ecx: 0x00,
        edx: 0x00,
        esi: 0x00,
        edi: 0x00,
        ebp: 0x00,
        eip: 0x100,
        esp: 0xfffe,
        eflags: 0x2,
        cs: PSP_SEGMENT,
        ss: PSP_SEGMENT,
        es: PSP_SEGMENT,
        ds: PSP_SEGMENT,
        fs: PSP_SEGMENT,
        gs: PSP_SEGMENT,
    });
}

/// Load an MZ EXE: parse the header, load the program image after the PSP,
/// apply segment relocations, and enter the VM with CS:IP and SS:SP from the header.
fn load_mz_exe(file_handle: Handle) -> ! {
    // Read the MZ header
    let mut mz = MzHeader::default();
    let mz_bytes = unsafe {
        core::slice::from_raw_parts_mut(
            &mut mz as *mut MzHeader as *mut u8,
            core::mem::size_of::<MzHeader>(),
        )
    };
    let _ = read_sync(file_handle, mz_bytes, 0);

    // Calculate image size (total file size minus header)
    let header_size = mz.header_paragraphs as u32 * 16;
    let file_image_size = if mz.last_page_bytes == 0 {
        mz.total_pages as u32 * 512
    } else {
        (mz.total_pages as u32 - 1) * 512 + mz.last_page_bytes as u32
    } - header_size;

    setup_dos_memory();

    setup_psp();

    // Load the program image at PSP_BASE + 0x100 (after the 256-byte PSP)
    let load_addr = PSP_BASE + 0x100;
    read_file_into(file_handle, load_addr, file_image_size, header_size);

    // Apply relocations: each entry points to a 16-bit word that needs
    // the load segment added to it
    let reloc_count = mz.relocation_count as u32;
    let mut reloc_file_offset = mz.relocation_offset as u32;
    for _ in 0..reloc_count {
        let mut reloc = MzRelocation::default();
        let reloc_bytes = unsafe {
            core::slice::from_raw_parts_mut(
                &mut reloc as *mut MzRelocation as *mut u8,
                core::mem::size_of::<MzRelocation>(),
            )
        };
        let _ = read_sync(file_handle, reloc_bytes, reloc_file_offset);
        reloc_file_offset += 4;

        // The fixup address in flat memory
        let fixup_addr = load_addr + reloc.segment as u32 * 16 + reloc.offset as u32;
        let ptr = fixup_addr as *mut u16;
        unsafe {
            let prev = core::ptr::read_volatile(ptr);
            core::ptr::write_volatile(ptr, prev.wrapping_add(PROGRAM_SEGMENT as u16));
        }
    }

    let _ = close_sync(file_handle);

    // MZ EXE: CS and SS are relative to the load segment (PROGRAM_SEGMENT)
    compat_start(VMRegisters {
        eax: 0x00,
        ebx: 0x00,
        ecx: 0x00,
        edx: 0x00,
        esi: 0x00,
        edi: 0x00,
        ebp: 0x00,
        eip: mz.initial_ip as u32,
        esp: mz.initial_sp as u32,
        eflags: 0x2,
        cs: PROGRAM_SEGMENT + mz.initial_cs as u32,
        ss: PROGRAM_SEGMENT + mz.initial_ss as u32,
        es: PSP_SEGMENT,
        ds: PSP_SEGMENT,
        fs: PSP_SEGMENT,
        gs: PSP_SEGMENT,
    });
}

/// Helper: read `size` bytes from `file_handle` at `file_offset` into `dest_addr`.
fn read_file_into(file_handle: Handle, dest_addr: u32, size: u32, file_offset: u32) {
    let dest = unsafe { core::slice::from_raw_parts_mut(dest_addr as *mut u8, size as usize) };
    let mut read_offset: u32 = 0;
    while read_offset < size {
        let chunk = &mut dest[read_offset as usize..size as usize];
        match read_sync(file_handle, chunk, file_offset + read_offset) {
            Ok(bytes_read) => {
                read_offset += bytes_read;
            }
            Err(_) => {
                idos_api::syscall::exec::terminate(0xfd);
            }
        }
    }
}

fn compat_start(mut vm_regs: VMRegisters) -> ! {
    let mut termios = Termios::default();
    let _ = ioctl_sync(
        STDIN,
        idos_api::io::termios::TCGETS,
        &mut termios as *mut Termios as u32,
        core::mem::size_of::<Termios>() as u32,
    )
    .unwrap();

    unsafe {
        TERMIOS_ORIG = termios.clone();
    }
    termios.lflags &= !(idos_api::io::termios::ECHO | idos_api::io::termios::ICANON);
    let _ = ioctl_sync(
        STDIN,
        idos_api::io::termios::TCSETS,
        &termios as *const Termios as u32,
        core::mem::size_of::<Termios>() as u32,
    );

    let stdaux = create_file_handle();
    let _ = open_sync(stdaux, "DEV:\\COM1", 0);

    let kbd = create_file_handle();
    let _ = open_sync(kbd, "DEV:\\KEYBOARD", 0);
    KBD_HANDLE.store(kbd.as_u32(), core::sync::atomic::Ordering::Relaxed);

    loop {
        let irq_mask = unsafe {
            if VM86_IF { VM86_IRQ_MASK } else { 0 }
        };
        let exit_reason = idos_api::syscall::exec::enter_8086(&mut vm_regs, irq_mask);

        match exit_reason {
            idos_api::compat::VM86_EXIT_GPF => unsafe {
                if !handle_fault(&mut vm_regs) {
                    break;
                }
            },
            idos_api::compat::VM86_EXIT_DEBUG => {
                // Hardware interrupt delivery — TF was set by the kernel
                // Clear TF from the saved eflags so we don't keep trapping
                vm_regs.eflags &= !0x100;
                // TODO: deliver pending virtual interrupts
            }
            _ => break,
        }
    }

    exit(0);
}

fn exit(code: u32) -> ! {
    // reset termios
    unsafe {
        let _ = ioctl_sync(
            STDIN,
            idos_api::io::termios::TCSETS,
            &raw const TERMIOS_ORIG as *const Termios as u32,
            core::mem::size_of::<Termios>() as u32,
        );
    }

    idos_api::syscall::exec::terminate(code)
}

unsafe fn handle_fault(vm_regs: &mut VMRegisters) -> bool {
    let op_ptr = ((vm_regs.cs << 4) + vm_regs.eip) as *const u8;
    match *op_ptr {
        0x9c => {
            // PUSHF — push flags onto the v86 stack
            vm_regs.esp = (vm_regs.esp & 0xffff).wrapping_sub(2);
            let stack_addr = (vm_regs.ss << 4) + (vm_regs.esp & 0xffff);
            core::ptr::write_volatile(stack_addr as *mut u16, vm_regs.eflags as u16);
            vm_regs.eip += 1;
        }
        0x9d => {
            // POPF — pop flags from the v86 stack
            let stack_addr = (vm_regs.ss << 4) + (vm_regs.esp & 0xffff);
            let flags = core::ptr::read_volatile(stack_addr as *const u16) as u32;
            // Preserve VM flag and IOPL, update the rest
            vm_regs.eflags = (vm_regs.eflags & 0xFFF20000) | (flags & 0x0000FFFF);
            vm_regs.esp = (vm_regs.esp & 0xffff).wrapping_add(2);
            vm_regs.eip += 1;
        }
        0xcd => {
            // INT nn
            let irq = *op_ptr.add(1);
            handle_interrupt(irq, vm_regs);
            vm_regs.eip += 2;
        }
        0xcf => {
            // IRET — pop IP, CS, FLAGS from v86 stack
            let stack_addr = (vm_regs.ss << 4) + (vm_regs.esp & 0xffff);
            let ip = core::ptr::read_volatile(stack_addr as *const u16) as u32;
            let cs = core::ptr::read_volatile((stack_addr + 2) as *const u16) as u32;
            let flags = core::ptr::read_volatile((stack_addr + 4) as *const u16) as u32;
            vm_regs.eip = ip;
            vm_regs.cs = cs;
            vm_regs.eflags = (vm_regs.eflags & 0xFFF20000) | (flags & 0x0000FFFF);
            vm_regs.esp = (vm_regs.esp & 0xffff).wrapping_add(6);
            return true; // don't advance EIP, we set it directly
        }
        0xf4 => {
            // HLT — stop execution
            return false;
        }
        0xfa => {
            // CLI
            VM86_IF = false;
            vm_regs.eip += 1;
        }
        0xfb => {
            // STI
            VM86_IF = true;
            vm_regs.eip += 1;
        }
        _ => {
            return false;
        }
    }

    true
}

/// BIOS keyboard services (INT 16h)
fn bios_keyboard(regs: &mut VMRegisters) {
    match regs.ah() {
        0x00 | 0x10 => {
            // AH=00/10: Blocking read — wait for a keypress
            // Returns AH=IBM scancode, AL=ASCII character
            loop {
                if let Some((scancode, ascii)) = read_next_key() {
                    regs.set_ah(scancode);
                    regs.set_al(ascii);
                    return;
                }
                idos_api::syscall::exec::yield_coop();
            }
        }
        0x01 | 0x11 => {
            // AH=01/11: Check if key available (non-blocking)
            // If key available: ZF=0, AX=key data (key remains in buffer)
            // If no key: ZF=1
            if let Some((scancode, ascii)) = peek_next_key() {
                regs.set_ah(scancode);
                regs.set_al(ascii);
                // Clear ZF to indicate key available
                regs.eflags &= !0x40;
            } else {
                // Set ZF to indicate no key
                regs.eflags |= 0x40;
                // Yield so the keyboard driver can deliver data
                idos_api::syscall::exec::yield_coop();
            }
        }
        0x02 | 0x12 => {
            // AH=02/12: Get shift key status
            // Return 0 for now (no modifier keys pressed)
            regs.set_al(0);
        }
        _ => {}
    }
}

/// Read and consume the next key press from the keyboard device.
/// Returns (IBM_scancode, ASCII) or None if no key press is available.
fn read_next_key() -> Option<(u8, u8)> {
    // Check lookahead first (populated by peek_next_key)
    unsafe {
        if let Some(key) = KEY_LOOKAHEAD.take() {
            return Some(key);
        }
    }
    let kbd = Handle::new(KBD_HANDLE.load(core::sync::atomic::Ordering::Relaxed));
    let mut buf = [0u8; 2];
    loop {
        match idos_api::io::sync::read_sync(kbd, &mut buf, 0) {
            Ok(2) => {
                if buf[0] == 1 {
                    // Key press
                    if let Some(result) = keycode_to_bios(buf[1]) {
                        return Some(result);
                    }
                    // Modifier or unmapped key, skip
                }
                // Key release (buf[0] == 2), skip
            }
            _ => return None,
        }
    }
}

/// Peek at the next key press without consuming it.
/// Since the keyboard device doesn't support peek, we read into a small
/// lookahead buffer that read_next_key also checks.
fn peek_next_key() -> Option<(u8, u8)> {
    unsafe {
        if let Some(ref key) = KEY_LOOKAHEAD {
            return Some(*key);
        }
    }
    if let Some(key) = read_next_key() {
        unsafe { KEY_LOOKAHEAD = Some(key); }
        Some(key)
    } else {
        None
    }
}

static mut KEY_LOOKAHEAD: Option<(u8, u8)> = None;

/// Map an IDOS KeyCode byte to (IBM_scancode, ASCII).
/// KeyCode values match kernel/src/hardware/ps2/keycodes.rs KeyCode enum.
fn keycode_to_bios(keycode: u8) -> Option<(u8, u8)> {
    // KeyCode -> (IBM scancode, ASCII lowercase)
    // IBM scancodes from the standard scan code set 1 make codes
    let (scancode, ascii) = match keycode {
        0x08 => (0x0E, 0x08u8), // Backspace
        0x09 => (0x0F, 0x09),   // Tab
        0x0D => (0x1C, 0x0D),   // Enter
        0x1B => (0x01, 0x1B),   // Escape
        0x20 => (0x39, 0x20),   // Space

        // Numbers 0-9
        0x30 => (0x0B, b'0'), 0x31 => (0x02, b'1'), 0x32 => (0x03, b'2'),
        0x33 => (0x04, b'3'), 0x34 => (0x05, b'4'), 0x35 => (0x06, b'5'),
        0x36 => (0x07, b'6'), 0x37 => (0x08, b'7'), 0x38 => (0x09, b'8'),
        0x39 => (0x0A, b'9'),

        // Letters A-Z (lowercase ASCII)
        0x41 => (0x1E, b'a'), 0x42 => (0x30, b'b'), 0x43 => (0x2E, b'c'),
        0x44 => (0x20, b'd'), 0x45 => (0x12, b'e'), 0x46 => (0x21, b'f'),
        0x47 => (0x22, b'g'), 0x48 => (0x23, b'h'), 0x49 => (0x17, b'i'),
        0x4A => (0x24, b'j'), 0x4B => (0x25, b'k'), 0x4C => (0x26, b'l'),
        0x4D => (0x32, b'm'), 0x4E => (0x31, b'n'), 0x4F => (0x18, b'o'),
        0x50 => (0x19, b'p'), 0x51 => (0x10, b'q'), 0x52 => (0x13, b'r'),
        0x53 => (0x1F, b's'), 0x54 => (0x14, b't'), 0x55 => (0x16, b'u'),
        0x56 => (0x2F, b'v'), 0x57 => (0x11, b'w'), 0x58 => (0x2D, b'x'),
        0x59 => (0x15, b'y'), 0x5A => (0x2C, b'z'),

        // Punctuation
        0x2C => (0x33, b','), 0x2D => (0x0C, b'-'), 0x2E => (0x34, b'.'),
        0x2F => (0x35, b'/'), 0x3A => (0x27, b';'), 0x3B => (0x28, b'\''),
        0x3D => (0x0D, b'='), 0x5B => (0x1A, b'['), 0x5C => (0x2B, b'\\'),
        0x5D => (0x1B, b']'), 0x5F => (0x29, b'`'),

        // Arrow keys (extended, no ASCII)
        0x21 => (0x4B, 0x00), // Left
        0x22 => (0x48, 0x00), // Up
        0x23 => (0x4D, 0x00), // Right
        0x24 => (0x50, 0x00), // Down

        0x07 => (0x53, 0x00), // Delete

        // Modifiers and unmapped keys return None
        _ => return None,
    };
    Some((scancode, ascii))
}

fn handle_interrupt(irq: u8, vm_regs: &mut VMRegisters) {
    match irq {
        0x16 => {
            // BIOS keyboard services
            bios_keyboard(vm_regs);
        }
        0x21 => {
            // DOS API
            dos_api(vm_regs);
        }

        // TODO: jump to the value in the IVT, or fail if there is no irq
        _ => (),
    }
}

fn dos_api(vm_regs: &mut VMRegisters) {
    match vm_regs.ah() {
        0x00 => terminate(vm_regs),
        0x01 => read_stdin_with_echo(vm_regs),
        0x02 => output_char_to_stdout(vm_regs),
        0x04 => write_char_stdaux(vm_regs),
        0x09 => print_string(vm_regs),
        0x25 => set_interrupt_vector(vm_regs),
        0x30 => get_dos_version(vm_regs),
        0x35 => get_interrupt_vector(vm_regs),
        0x3D => open_file(vm_regs),
        0x3E => close_file(vm_regs),
        0x40 => write_file(vm_regs),
        0x42 => seek_file(vm_regs),
        0x44 => ioctl(vm_regs),
        0x4A => resize_memory(vm_regs),
        0x4C => terminate_with_code(vm_regs),
        0x63 => get_dbcs_table(vm_regs),
        0x66 => get_global_code_page(vm_regs),
        0x68 => commit_file(vm_regs),
        _ => {
            // Unsupported — set carry flag to indicate error
            vm_regs.eflags |= 1;
        }
    }
}

/// AH=0x00 - Terminate the current program
/// Restores the interrupt vectors 0x22, 0x23, 0x24. Frees memory allocated to
/// the current program, but does not close FCBs.
/// Input:
///     CS points to the PSP
/// Output:
///     If a termination vector exists, set CS and IP to that vector
pub fn terminate(regs: &mut VMRegisters) {
    // TODO: Check PSP for parent segment
    // if has parent segment
    //   set cs to termination vector segment
    //   set eip to termination vector offset

    exit(1);
}

/// AH=0x01 - Read from STDIN and echo to STDOUT
/// Input:
///     None
/// Output:
///     AL = character from STDIN
pub fn read_stdin_with_echo(regs: &mut VMRegisters) {
    let stdin = idos_api::io::handle::Handle::new(0);
    let stdout = idos_api::io::handle::Handle::new(1);

    let mut buffer: [u8; 1] = [0; 1];

    match idos_api::io::sync::read_sync(stdin, &mut buffer, 0) {
        Ok(len) if len == 1 => {
            let _ = idos_api::io::sync::write_sync(stdout, &mut buffer, 0);
        }
        _ => (),
    }

    regs.set_al(buffer[0]);
}

/// AH=0x02 - Output single character to STDOUT
/// Input:
///     DL = character to output
/// Output:
///     None
pub fn output_char_to_stdout(regs: &mut VMRegisters) {
    let char = regs.dl();
    let buffer: [u8; 1] = [char];
    let stdout = idos_api::io::handle::Handle::new(1);
    let _ = idos_api::io::sync::write_sync(stdout, &buffer, 0);
}

/// AH=0x03 - Blocking character read from STDAUX (COM)
/// Input:
///     None
/// Output:
///     AL = character from STDAUX
pub fn read_char_stdaux(_regs: &mut VMRegisters) {}

/// AH=0x04 - Write character to STDAUX
/// Input:
///     DL = character to output
/// Output:
///     None
pub fn write_char_stdaux(regs: &mut VMRegisters) {
    let char = regs.dl();
    let buffer: [u8; 1] = [char];
    let stdaux = idos_api::io::handle::Handle::new(2);
    let _ = idos_api::io::sync::write_sync(stdaux, &buffer, 0);
}

/// AH=0x09 - Print a dollar-terminated string to STDOUT
/// Input:
///     DS:DX points to the string
/// Output:
///     None
pub fn print_string(regs: &mut VMRegisters) {
    let dx = regs.edx & 0xffff;
    let start_address = (regs.ds << 4) + dx;
    let start_ptr = start_address as *const u8;
    let search_len = 256.min(0x10000 - dx) as usize;
    let mut string_len = 0;
    while string_len < search_len {
        unsafe {
            if core::ptr::read_volatile(start_ptr.add(string_len)) == b'$' {
                break;
            }
        }
        string_len += 1;
    }
    let string_slice = unsafe { core::slice::from_raw_parts(start_ptr, string_len) };
    let stdout = idos_api::io::handle::Handle::new(1);
    let _ = idos_api::io::sync::write_sync(stdout, string_slice, 0);
}

/// AH=0x25 - Set interrupt vector
/// Input: AL=interrupt number, DS:DX=new handler address
fn set_interrupt_vector(regs: &mut VMRegisters) {
    let int_num = regs.al() as u32;
    let offset = regs.edx & 0xffff;
    let segment = regs.ds;
    // Write to the IVT at address 0000:(int_num * 4)
    let ivt_addr = (int_num * 4) as *mut u16;
    unsafe {
        core::ptr::write_volatile(ivt_addr, offset as u16);
        core::ptr::write_volatile(ivt_addr.add(1), segment as u16);
    }

    // If the program is hooking a hardware interrupt, record it in the IRQ mask.
    // INT 8-15 map to IRQ 0-7, INT 70-77 map to IRQ 8-15.
    let irq = match int_num {
        0x08..=0x0F => Some(int_num - 0x08),
        0x70..=0x77 => Some(int_num - 0x70 + 8),
        // INT 1Ch is the user timer hook, chained from INT 8 (IRQ 0)
        0x1C => Some(0),
        _ => None,
    };
    if let Some(irq_num) = irq {
        unsafe { VM86_IRQ_MASK |= 1 << irq_num; }
    }
}

/// AH=0x30 - Get DOS version
/// Returns AL=major, AH=minor, BH=OEM ID, BL:CX=serial
fn get_dos_version(regs: &mut VMRegisters) {
    regs.set_al(5);   // DOS 5.0
    regs.set_ah(0);
    regs.ebx = 0;     // OEM=IBM, serial=0
    regs.ecx = 0;
}

/// AH=0x35 - Get interrupt vector
/// Input: AL=interrupt number
/// Output: ES:BX=current handler address
fn get_interrupt_vector(regs: &mut VMRegisters) {
    let int_num = regs.al() as u32;
    let ivt_addr = (int_num * 4) as *const u16;
    unsafe {
        let offset = core::ptr::read_volatile(ivt_addr) as u32;
        let segment = core::ptr::read_volatile(ivt_addr.add(1)) as u32;
        regs.ebx = offset;
        regs.es = segment;
    }
}

/// AH=0x3D - Open file
/// Input: AL=access mode, DS:DX=ASCIIZ filename
/// Output: CF=0 AX=handle on success, CF=1 AX=error on failure
fn open_file(regs: &mut VMRegisters) {
    // Stub: return error (file not found)
    regs.eflags |= 1; // set CF
    regs.set_ax(0x02); // error 2 = file not found
}

/// AH=0x3E - Close file
/// Input: BX=file handle
/// Output: CF=0 on success
fn close_file(regs: &mut VMRegisters) {
    // Stub: always succeed
    regs.eflags &= !1; // clear CF
}

/// AH=0x40 - Write to file or device
/// Input: BX=file handle, CX=byte count, DS:DX=buffer
/// Output: CF=0 AX=bytes written on success
fn write_file(regs: &mut VMRegisters) {
    let handle = regs.ebx & 0xffff;
    let count = (regs.ecx & 0xffff) as usize;
    let dx = regs.edx & 0xffff;
    let buffer_addr = (regs.ds << 4) + dx;
    let buffer = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, count) };

    // DOS file handles: 0=stdin, 1=stdout, 2=stderr
    match handle {
        1 | 2 => {
            let stdout = idos_api::io::handle::Handle::new(1);
            let _ = idos_api::io::sync::write_sync(stdout, buffer, 0);
            regs.set_ax(count as u16);
            regs.eflags &= !1; // clear CF
        }
        3 => {
            // stdaux
            let stdaux = idos_api::io::handle::Handle::new(2);
            let _ = idos_api::io::sync::write_sync(stdaux, buffer, 0);
            regs.set_ax(count as u16);
            regs.eflags &= !1;
        }
        _ => {
            // Unknown handle — error
            regs.eflags |= 1;
            regs.set_ax(0x06); // error 6 = invalid handle
        }
    }
}

/// AH=0x42 - Seek (LSEEK)
/// Input: AL=origin, BX=handle, CX:DX=offset
/// Output: CF=0 DX:AX=new position on success
fn seek_file(regs: &mut VMRegisters) {
    // Stub: for device handles, return position 0
    regs.set_ax(0);
    regs.edx &= 0xffff0000;
    regs.eflags &= !1; // clear CF
}

/// AH=0x44 - IOCTL
/// AL=subfunction, BX=handle
fn ioctl(regs: &mut VMRegisters) {
    let subfunc = regs.al();
    match subfunc {
        0x00 => {
            // Get device information
            let handle = regs.ebx & 0xffff;
            let info: u16 = match handle {
                0 => 0x80D3, // stdin: device, stdin, stdout, NUL, isdev
                1 => 0x80D3, // stdout: same
                2 => 0x80D3, // stderr: same
                _ => 0x0000, // disk file
            };
            regs.edx = (regs.edx & 0xffff0000) | info as u32;
            regs.eflags &= !1;
        }
        _ => {
            regs.eflags |= 1;
            regs.set_ax(0x01); // error 1 = invalid function
        }
    }
}

/// AH=0x4A - Resize memory block
/// Input: BX=new size in paragraphs, ES=segment of block
/// Output: CF=0 on success, CF=1 BX=max available on failure
fn resize_memory(regs: &mut VMRegisters) {
    // Stub: always succeed — we pre-mapped enough memory
    regs.eflags &= !1; // clear CF
}

/// AH=0x4C - Terminate with return code
/// Input: AL=return code
fn terminate_with_code(regs: &mut VMRegisters) {
    let code = regs.al() as u32;
    exit(code);
}

/// AH=0x63 - Get DBCS lead byte table
/// Output: DS:SI=pointer to DBCS table
fn get_dbcs_table(regs: &mut VMRegisters) {
    // Return a pointer to a table that's just a terminator (0000).
    // We'll use two zero bytes somewhere safe in the PSP reserved area.
    // PSP offset 0x3C is reserved and we zeroed it, so point there.
    regs.ds = PSP_SEGMENT;
    regs.esi = 0x3C;
}

/// AH=0x66 - Get/set global code page
/// AL=01: get, AL=02: set
fn get_global_code_page(regs: &mut VMRegisters) {
    let subfunc = regs.al();
    match subfunc {
        0x01 => {
            // Get: BX=active code page, DX=system code page
            regs.ebx = (regs.ebx & 0xffff0000) | 437; // US English
            regs.edx = (regs.edx & 0xffff0000) | 437;
            regs.eflags &= !1;
        }
        _ => {
            regs.eflags &= !1; // just succeed
        }
    }
}

/// AH=0x68 - Commit/flush file
/// Input: BX=handle
fn commit_file(regs: &mut VMRegisters) {
    // Stub: always succeed
    regs.eflags &= !1;
}
