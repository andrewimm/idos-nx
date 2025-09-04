#![no_std]
#![no_main]

extern crate idos_api;
extern crate idos_sdk;

use core::sync::atomic::Ordering;

use idos_api::{
    io::{termios::Termios, AsyncOp, Handle, ASYNC_OP_READ},
    syscall::{exec::futex_wait_u32, io::append_io_op},
};

#[no_mangle]
pub extern "C" fn main() {
    let stdin = Handle::new(0);
    let serial = idos_api::syscall::io::create_file_handle();
    idos_api::io::sync::open_sync(serial, "DEV:\\COM1").unwrap();

    let mut termios = Termios::default();
    let _ = idos_api::io::sync::ioctl_sync(
        stdin,
        idos_api::io::termios::TCGETS,
        &mut termios as *mut Termios as u32,
        core::mem::size_of::<Termios>() as u32,
    )
    .unwrap();

    let saved_termios = termios.clone();
    termios.lflags &= !(idos_api::io::termios::ECHO | idos_api::io::termios::ICANON);
    let _ = idos_api::io::sync::ioctl_sync(
        stdin,
        idos_api::io::termios::TCSETS,
        &termios as *const Termios as u32,
        core::mem::size_of::<Termios>() as u32,
    );

    // set up the graphics buffer
    let mut graphics_mode = idos_api::io::termios::GraphicsMode {
        width: 200,
        height: 200,
        bpp_flags: 8,
        framebuffer: 0,
    };
    let _ = idos_api::io::sync::ioctl_sync(
        stdin,
        idos_api::io::termios::TSETGFX,
        &mut graphics_mode as *mut idos_api::io::termios::GraphicsMode as u32,
        core::mem::size_of::<idos_api::io::termios::GraphicsMode>() as u32,
    );

    let framebuffer_phys = graphics_mode.framebuffer;
    let framebuffer_vaddr =
        idos_api::syscall::memory::map_memory(None, 200 * 200, Some(framebuffer_phys)).unwrap();

    let framebuffer_ptr = framebuffer_vaddr as *mut u8;
    let framebuffer_size = 200 * 200;
    let framebuffer = unsafe { core::slice::from_raw_parts_mut(framebuffer_ptr, framebuffer_size) };

    for byte in framebuffer.iter_mut() {
        *byte = 0x0c;
    }

    let mut read_buffer: [u8; 1] = [0; 1];
    let mut read_op = AsyncOp::new(
        ASYNC_OP_READ,
        &mut read_buffer[0] as *mut u8 as u32,
        read_buffer.len() as u32,
        0,
    );
    append_io_op(stdin, &read_op, None);

    idos_api::io::sync::write_sync(serial, b"Begin game", 0).unwrap();

    let mut pixel_offset = 408;

    'gameloop: loop {
        if read_op.is_complete() {
            let return_value = read_op.return_value.load(Ordering::SeqCst);
            if return_value & 0x80000000 == 0 {
                let mut i = 0;
                while i < return_value {
                    let byte = read_buffer[i as usize];
                    if byte == b'q' {
                        // exit on 'q'
                        break 'gameloop;
                    } else {
                        // draw a pixel at a random position
                        framebuffer[pixel_offset] = 0x0f;
                        pixel_offset += 10;
                    }
                    i += 1;
                }
            }

            read_op = AsyncOp::new(
                ASYNC_OP_READ,
                &mut read_buffer[0] as *mut u8 as u32,
                read_buffer.len() as u32,
                0,
            );
            append_io_op(stdin, &read_op, None);
        }

        futex_wait_u32(&read_op.signal, 0, None);
    }
    idos_api::io::sync::write_sync(serial, b"Exit game\n", 0).unwrap();

    let _ = idos_api::io::sync::ioctl_sync(stdin, idos_api::io::termios::TSETTEXT, 0, 0);

    let _ = idos_api::io::sync::ioctl_sync(
        stdin,
        idos_api::io::termios::TCSETS,
        &saved_termios as *const Termios as u32,
        core::mem::size_of::<Termios>() as u32,
    );
}
