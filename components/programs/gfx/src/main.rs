#![no_std]
#![no_main]

extern crate idos_api;
extern crate idos_sdk;

use idos_api::{
    compat::VMRegisters,
    io::{
        read_message_op,
        sync::{read_sync, write_sync},
        Handle, Message,
    },
    syscall::{
        exec::enter_8086,
        io::{append_io_op, block_on_wake_set, create_message_queue_handle, create_wake_set},
    },
};

#[no_mangle]
pub extern "C" fn main() {
    let message_queue = create_message_queue_handle();
    let mut incoming_message = Message::empty();

    // identity-map the bottom page
    idos_api::syscall::memory::map_memory(
        Some(0x0000_0000),
        0x1000, // 4KB
        Some(0x0000_0000),
    )
    .unwrap();

    // identity-map BIOS code (0xA0000 - 0xFFFFF)
    let mut frame = 0x000a_0000;
    while frame < 0x0010_0000 {
        idos_api::syscall::memory::map_memory(Some(frame), 0x1000, Some(frame)).unwrap();
        frame += 0x1000;
    }

    // map a page to store the stack in 8086 mode
    let stack_frame =
        idos_api::syscall::memory::map_memory(Some(0x0000_8000), 0x1000, None).unwrap();
    let stack_top = stack_frame + 0x1000;

    let mut vm_regs = VMRegisters {
        eax: 0x00,
        ebx: 0x00,
        ecx: 0x00,
        edx: 0x00,
        esi: 0x00,
        edi: 0x00,
        ebp: 0x00,
        eip: 0x00,
        esp: stack_top,
        eflags: 0x2,
        cs: 0,
        ss: 0,
        es: 0,
        ds: 0,
        fs: 0,
        gs: 0,
    };

    let wake_set = create_wake_set();
    let mut message_read = read_message_op(&mut incoming_message);
    append_io_op(message_queue, &message_read, Some(wake_set));

    loop {
        if message_read.is_complete() {
            match incoming_message.message_type {
                1 => {
                    // set VGA mode
                    set_vga_mode(0x13, &mut vm_regs, stack_top);
                }
                _ => (),
            }

            message_read = read_message_op(&mut incoming_message);
            append_io_op(message_queue, &message_read, Some(wake_set));
        }

        block_on_wake_set(wake_set, None);
    }
}

fn set_vga_mode(mode: u8, regs: &mut VMRegisters, stack_top: u32) {
    let int_10_ip: u16 = unsafe { core::ptr::read_volatile(0x0000_0040 as *const u16) };
    let int_10_segment: u16 = unsafe { core::ptr::read_volatile(0x0000_0042 as *const u16) };
    regs.eax = mode as u32;
    regs.eip = int_10_ip as u32;
    regs.cs = int_10_segment as u32;

    regs.esp = stack_top;
    unsafe {
        // push flags
        regs.esp -= 2;
        *(regs.esp as *mut u16) = 0;
        // push cs
        regs.esp -= 2;
        *(regs.esp as *mut u16) = 0;
        // push ip
        regs.esp -= 2;
        *(regs.esp as *mut u16) = 0;
    }

    loop {
        enter_8086(regs);

        unsafe {
            let mut op_ptr = ((regs.cs << 4) + regs.eip) as *const u8;
            match *op_ptr {
                0x9c => {
                    // PUSHF
                    regs.esp = regs.esp.wrapping_sub(2) & 0xffff;
                    *(regs.esp as *mut u16) = regs.eflags as u16;
                    regs.eip += 1;
                }
                0x9d => {
                    // POPF
                    let flags = *(regs.esp as *mut u16);
                    regs.esp = regs.esp.wrapping_add(2) & 0xffff;
                    regs.eflags = (flags as u32) | 0x20200;
                    regs.eip += 1;
                }
                0xcf => {
                    // IRET
                    // exit the loop, this marks the end of the interrupt
                    break;
                }
                0xfa => {
                    // CLI
                    regs.eip += 1;
                }
                0xfb => {
                    // STI
                    regs.eip += 1;
                }
                _ => panic!("Unhandled 8086 instruction"),
            }
        }
    }

    unsafe {
        let base = 0x000a_0000 as *mut u8;
        for row in 0..8 {
            for i in 0..256 {
                core::ptr::write_volatile(base.add(row * 320 + i), i as u8);
            }
        }
    }
}
