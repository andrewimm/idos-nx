//! In order for DOS programs to run, IDOS needs to put a DOS compatibility
//! layer in higher userspace memory. This 32-bit code runs in a loop, entering
//! a 8086 VM before returning on an interrupt or GPF fault.

#![no_std]
#![no_main]
#![feature(lang_items)]

extern crate idos_api;

pub mod panic;
use core::arch::{asm, global_asm};

use idos_api::compat::VMRegisters;

global_asm!(
    r#"
.global _start

_start:
    mov edi, [esp]
    lea esi, [esp + 4]
    push esi
    push edi
    push eax
    call compat_start
"#
);

#[no_mangle]
pub extern "C" fn compat_start(psp_segment: u32, _argc: u32, _argv: *const u32) {
    //env::init_args(argc, argv);

    let stdaux = idos_api::syscall::io::create_file_handle();
    let _ = idos_api::io::sync::open_sync(stdaux, "DEV:\\COM1");

    let mut vm_regs = VMRegisters {
        eax: 0xaa,
        ebx: 0xbb,
        ecx: 0xcc,
        edx: 0xdd,
        esi: 0xee,
        edi: 0xff,
        ebp: 0xab,
        eip: 0x100,
        esp: 0xfae,
        eflags: 0x2,
        cs: psp_segment as u32,
        ss: psp_segment as u32,
        es: psp_segment as u32,
        ds: psp_segment as u32,
        fs: psp_segment as u32,
        gs: psp_segment as u32,
    };

    loop {
        idos_api::syscall::exec::enter_8086(&mut vm_regs);

        unsafe {
            if !handle_fault(&mut vm_regs) {
                break;
            }
        }
    }

    idos_api::syscall::exec::terminate(0)
}

unsafe fn handle_fault(vm_regs: &mut VMRegisters) -> bool {
    let mut op_ptr = ((vm_regs.cs << 4) + vm_regs.eip) as *const u8;
    // TODO: check prefix
    match *op_ptr {
        0x9c => { // PUSHF
        }
        0x9d => { // POPF
        }
        0xcd => {
            // INT
            let irq = *op_ptr.add(1);
            handle_interrupt(irq, vm_regs);
            vm_regs.eip += 2;
            return true;
        }
        0xcf => { // IRET
        }
        0xf4 => { // HLT
        }
        0xfa => { // CLI
        }
        0xfb => { // STI
        }
        _ => (),
    }

    false
}

fn handle_interrupt(irq: u8, vm_regs: &mut VMRegisters) {
    match irq {
        // So many interrupts to implement here...
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
        _ => {
            panic!("Unsupported API")
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

    idos_api::syscall::exec::terminate(1);
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
