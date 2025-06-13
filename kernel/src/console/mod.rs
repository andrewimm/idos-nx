use core::sync::atomic::{AtomicU32, Ordering};

use alloc::vec::Vec;
use idos_api::io::AsyncOp;
use spin::RwLock;

use crate::conman::{register_console_manager, InputBuffer};
use crate::io::async_io::ASYNC_OP_READ;
use crate::io::handle::Handle;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::sync::futex::{futex_wait, futex_wake};
use crate::task::actions::handle::{create_pipe_handles, open_message_queue, transfer_handle};
use crate::task::actions::io::{
    append_io_op, close_sync, driver_io_complete, read_sync, write_sync,
};
use crate::task::actions::lifecycle::{create_kernel_task, terminate};
use crate::task::actions::memory::map_memory;
use crate::task::actions::sync::{block_on_wake_set, create_wake_set};
use crate::task::id::TaskID;
use crate::task::memory::MemoryBacking;
use crate::task::messaging::Message;

use self::input::KeyAction;
use self::manager::ConsoleManager;

pub mod buffers;
pub mod console;
//pub mod driver;
pub mod input;
pub mod manager;

type ConsoleInputBuffer = InputBuffer<{ crate::conman::INPUT_BUFFER_SIZE }>;

pub fn manager_task() -> ! {
    let response_writer = Handle::new(0);

    let wake_set = create_wake_set();

    let input_buffer_addr = match register_console_manager(wake_set) {
        Ok(addr) => addr,
        Err(_) => {
            crate::kprintln!("Failed to register CONMAN");
            terminate(0);
        }
    };

    let keyboard_buffer_ptr = input_buffer_addr.as_ptr::<ConsoleInputBuffer>();
    let keyboard_buffer = unsafe { &*keyboard_buffer_ptr };
    let mouse_buffer = unsafe { &*(keyboard_buffer_ptr.add(1)) };

    let text_buffer_base = map_memory(
        None,
        0x1000,
        MemoryBacking::Direct(PhysicalAddress::new(0xb8000)),
    )
    .unwrap();

    const framebuffer_bytes: u32 = 800 * 600;
    const framebuffer_pages: u32 = (framebuffer_bytes + 0xfff) / 0x1000;

    let graphics_buffer_base = map_memory(
        Some(VirtualAddress::new(0x10_0000)),
        0x1000 * framebuffer_pages,
        MemoryBacking::Direct(PhysicalAddress::new(0xfd00_0000)),
    )
    .unwrap();
    let framebuffer = unsafe {
        core::slice::from_raw_parts_mut(
            graphics_buffer_base.as_ptr_mut::<u8>(),
            0x1000 * framebuffer_pages as usize,
        )
    };

    let mut mouse_x = 400;
    let mut mouse_y = 300;

    let mut conman = ConsoleManager::new(text_buffer_base);
    conman.add_console(); // create the first console (CON1)

    //conman.clear_screen();
    //conman.render_top_bar();

    let _ = write_sync(response_writer, &[0], 0);
    let _ = close_sync(response_writer);

    let messages_handle = open_message_queue();
    let mut incoming_message = Message::empty();

    let mut message_read = AsyncOp::new(
        ASYNC_OP_READ,
        &mut incoming_message as *mut Message as u32,
        core::mem::size_of::<Message>() as u32,
        0,
    );
    let _ = append_io_op(messages_handle, &message_read, Some(wake_set));

    let mut last_action_type: u8 = 0;
    loop {
        draw_desktop(framebuffer);

        draw_window(framebuffer, 40, 40, 480, 320);
        loop {
            // read input actions and pass them to the current console
            let next_action = match keyboard_buffer.read() {
                Some(action) => action,
                None => break,
            };
            if last_action_type == 0 {
                last_action_type = next_action;
            } else {
                match KeyAction::from_raw(last_action_type, next_action) {
                    Some(action) => {
                        conman.handle_key_action(action);
                    }
                    None => (),
                }
                last_action_type = 0;
            }
        }

        loop {
            let next_action = match mouse_buffer.read() {
                Some(action) => action,
                None => break,
            };
            crate::kprintln!("DRAW MOUSE");
            draw_mouse(framebuffer, mouse_x, mouse_y);
        }

        if message_read.is_complete() {
            let sender = message_read.return_value.load(Ordering::SeqCst);
            let request_id = incoming_message.unique_id;
            match conman.handle_request(&incoming_message) {
                Some(result) => driver_io_complete(request_id, result),
                None => (),
            }

            message_read = AsyncOp::new(
                ASYNC_OP_READ,
                &mut incoming_message as *mut Message as u32,
                core::mem::size_of::<Message>() as u32,
                0,
            );
            let _ = append_io_op(messages_handle, &message_read, Some(wake_set));
        }

        conman.update_cursor();
        //conman.update_clock();

        block_on_wake_set(wake_set, Some(1000));
    }
}

pub fn init_console() {
    let (response_reader, response_writer) = create_pipe_handles();
    let driver_task = create_kernel_task(manager_task, Some("CONMAN"));
    transfer_handle(response_writer, driver_task);

    let _ = read_sync(response_reader, &mut [0u8], 0);
    let _ = close_sync(response_reader);
}

pub fn console_ready() {
    //crate::command::start_command(0);
}

fn draw_desktop(framebuffer: &mut [u8]) {
    const TOP_BAR_HEIGHT: usize = 24;
    const DISPLAY_WIDTH: usize = 800;
    const DISPLAY_HEIGHT: usize = 600;

    // draw the top bar
    for y in 0..(TOP_BAR_HEIGHT - 2) {
        let offset = y * DISPLAY_WIDTH;
        for x in 0..DISPLAY_WIDTH {
            framebuffer[offset + x] = 0x12;
        }
    }
    for x in 0..DISPLAY_WIDTH {
        framebuffer[DISPLAY_WIDTH * (TOP_BAR_HEIGHT - 2) + x] = 0x5b;
    }
    for x in 0..DISPLAY_WIDTH {
        framebuffer[DISPLAY_WIDTH * (TOP_BAR_HEIGHT - 1) + x] = 0x5b;
    }

    // clear the rest of the desktop

    for y in TOP_BAR_HEIGHT..DISPLAY_HEIGHT {
        let offset = y * DISPLAY_WIDTH;
        for x in 0..DISPLAY_WIDTH {
            framebuffer[offset + x] = 0x14;
        }
    }
}

fn draw_window(
    framebuffer: &mut [u8],
    window_x: u32,
    window_y: u32,
    inner_width: u32,
    inner_height: u32,
) {
    const BORDER_WIDTH: usize = 2;
    let total_width: usize = inner_width as usize + BORDER_WIDTH * 2;

    let mut offset = ((window_y + 24) * 800 + window_x) as usize;

    for _ in 0..20 {
        for x in 0..total_width {
            framebuffer[offset + x] = 0x1d;
        }
        offset += 800;
    }

    for _ in 0..inner_height {
        framebuffer[offset] = 0x1d;
        framebuffer[offset + 1] = 0x1d;

        for x in 2..(total_width - 2) {
            framebuffer[offset + x] = 0x13;
        }

        framebuffer[offset + total_width - 2] = 0x1d;
        framebuffer[offset + total_width - 1] = 0x1d;

        offset += 800;
    }

    for x in 0..total_width {
        framebuffer[offset + x] = 0x1d;
    }
    offset += 800;
    for x in 0..total_width {
        framebuffer[offset + x] = 0x1d;
    }
}

fn draw_mouse(framebuffer: &mut [u8], mouse_x: u32, mouse_y: u32) {
    let offset = (mouse_y * 800 + mouse_x) as usize;
    for row in 0..16 {
        let row_offset = offset + row * 800;
        let mut cursor_row = CURSOR[row];
        let mut shadow = false;
        for col in 0..16 {
            if cursor_row & 1 != 0 {
                shadow = true;
                framebuffer[row_offset + col] = 0x0f;
            } else if shadow {
                shadow = false;
                framebuffer[row_offset + col] = 0x13;
            } else {
                shadow = false;
            }
            cursor_row = cursor_row >> 1;
        }
    }
}

const CURSOR: [u16; 16] = [
    0b0000000000000001,
    0b0000000000000011,
    0b0000000000000111,
    0b0000000000001111,
    0b0000000000011111,
    0b0000000000111111,
    0b0000000001111111,
    0b0000000011111111,
    0b0000000111111111,
    0b0000001111111111,
    0b0000011111111111,
    0b0000000001111111,
    0b0000000001100111,
    0b0000000001100011,
    0b0000000011000001,
    0b0000000011000000,
];
