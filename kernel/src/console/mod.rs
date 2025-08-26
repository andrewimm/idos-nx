use core::sync::atomic::Ordering;

use idos_api::io::AsyncOp;

use crate::conman::{register_console_manager, InputBuffer};
use crate::console::graphics::font::psf::PsfFont;
use crate::console::graphics::font::Font;
use crate::graphics::{get_vbe_mode_info, set_display_start_point, set_vbe_mode, VbeModeInfo};
use crate::io::async_io::ASYNC_OP_READ;
use crate::io::handle::Handle;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::task::actions::handle::{create_pipe_handles, open_message_queue, transfer_handle};
use crate::task::actions::io::{close_sync, driver_io_complete, read_sync, send_io_op, write_sync};
use crate::task::actions::lifecycle::{create_kernel_task, terminate};
use crate::task::actions::memory::map_memory;
use crate::task::actions::sync::{block_on_wake_set, create_wake_set};
use crate::task::id::TaskID;
use crate::task::memory::MemoryBacking;
use crate::task::messaging::Message;
use crate::time::system::get_system_ticks;

use self::graphics::framebuffer::Framebuffer;
use self::input::KeyAction;
use self::manager::ConsoleManager;

pub mod buffers;
pub mod console;
pub mod driver;
pub mod input;
pub mod manager;

pub mod graphics;

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

    let mut vbe_mode_info: VbeModeInfo = VbeModeInfo::default();
    get_vbe_mode_info(&mut vbe_mode_info, 0x0103);
    set_vbe_mode(0x0103);

    let framebuffer_bytes = (vbe_mode_info.pitch as u32) * (vbe_mode_info.height as u32);
    let framebuffer_pages = (framebuffer_bytes + 0xfff) / 0x1000;

    let graphics_buffer_base = map_memory(
        None,
        0x1000 * framebuffer_pages,
        MemoryBacking::Direct(PhysicalAddress::new(vbe_mode_info.framebuffer)),
    )
    .unwrap();

    let mut fb = Framebuffer {
        width: vbe_mode_info.width,
        height: vbe_mode_info.height,
        stride: vbe_mode_info.pitch,
        buffer: graphics_buffer_base,
    };

    {
        let buffer = fb.get_buffer_mut();
        for row in 0..vbe_mode_info.height as usize {
            let offset = row * vbe_mode_info.width as usize;
            for col in 0..vbe_mode_info.width as usize {
                buffer[offset + col] = if (row ^ col) & 2 == 0 { 0x00 } else { 0x0f };
            }
        }
    }

    let console_font =
        graphics::font::psf::PsfFont::from_file("C:\\TERM14.PSF").expect("Failed to load font");

    let mut mouse_x = vbe_mode_info.width as u32 / 2;
    let mut mouse_y = vbe_mode_info.height as u32 / 2;
    let mut mouse_read: [u8; 3] = [0, 0, 0];
    let mut mouse_read_index = 0;

    let mut conman = ConsoleManager::new();
    let con1 = conman.add_console(); // create the first console (CON1)

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
    let _ = send_io_op(messages_handle, &message_read, Some(wake_set));

    let mut last_action_type: u8 = 0;
    loop {
        draw_desktop(&fb, &console_font);

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
            let next_mouse_byte = match mouse_buffer.read() {
                Some(byte) => byte,
                None => break,
            };
            mouse_read[mouse_read_index] = next_mouse_byte;
            mouse_read_index += 1;
            if mouse_read_index == 1 {
                if next_mouse_byte & 0x08 == 0 {
                    mouse_read_index = 0; // first byte is not a valid mouse packet
                }
            } else if mouse_read_index == 3 {
                // we have a complete mouse packet
                let mut dx = mouse_read[1] as u32;
                let mut dy = mouse_read[2] as u32;
                if mouse_read[0] & 0x10 != 0 {
                    dx |= 0xffffff00;
                }
                if mouse_read[0] & 0x20 != 0 {
                    dy |= 0xffffff00;
                }
                let mouse_x_next = mouse_x as i32 + dx as i32;
                let mouse_y_next = mouse_y as i32 - dy as i32;
                if mouse_x_next < 0 {
                    mouse_x = 0;
                } else if mouse_x_next >= vbe_mode_info.width as i32 {
                    mouse_x = vbe_mode_info.width as u32 - 1;
                } else {
                    mouse_x = mouse_x_next as u32;
                }
                if mouse_y_next < 0 {
                    mouse_y = 0;
                } else if mouse_y_next >= vbe_mode_info.height as i32 {
                    mouse_y = vbe_mode_info.height as u32 - 1;
                } else {
                    mouse_y = mouse_y_next as u32;
                }
                mouse_read_index = 0; // reset for the next packet
            }
        }

        if message_read.is_complete() {
            let sender = TaskID::new(message_read.return_value.load(Ordering::SeqCst));
            let request_id = incoming_message.unique_id;
            match conman.handle_request(sender, &incoming_message) {
                Some(result) => driver_io_complete(request_id, result),
                None => (),
            }

            message_read = AsyncOp::new(
                ASYNC_OP_READ,
                &mut incoming_message as *mut Message as u32,
                core::mem::size_of::<Message>() as u32,
                0,
            );
            let _ = send_io_op(messages_handle, &message_read, Some(wake_set));
        }

        conman.draw_window(con1, &mut fb, &console_font);

        loop {
            let _next_action = match mouse_buffer.read() {
                Some(action) => action,
                None => break,
            };
        }
        draw_mouse(&mut fb, mouse_x, mouse_y);

        block_on_wake_set(wake_set, None);
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
    crate::command::start_command(0);
}

fn draw_desktop(framebuffer: &Framebuffer, font: &PsfFont) {
    const TOP_BAR_HEIGHT: usize = 24;
    let display_width: usize = framebuffer.width as usize;
    let display_height: usize = framebuffer.height as usize;

    let raw_buffer = framebuffer.get_buffer_mut();

    // draw the top bar
    for y in 0..(TOP_BAR_HEIGHT - 2) {
        let offset = y * display_width;
        for x in 0..display_width {
            raw_buffer[offset + x] = 0x12;
        }
    }
    for x in 0..display_width {
        raw_buffer[display_width * (TOP_BAR_HEIGHT - 2) + x] = 0x5b;
    }
    for x in 0..display_width {
        raw_buffer[display_width * (TOP_BAR_HEIGHT - 1) + x] = 0x5b;
    }

    let topbar_text_y = (TOP_BAR_HEIGHT - 2 - font.get_height() as usize) / 2;
    font.draw_string(
        framebuffer,
        10,
        topbar_text_y as u16,
        "IDOS-NX".bytes(),
        0x0f,
    );
    // clear the rest of the desktop

    for y in TOP_BAR_HEIGHT..display_height {
        let offset = y * display_width;
        for x in 0..display_width {
            raw_buffer[offset + x] = 0x14;
        }
    }
}

fn draw_mouse(framebuffer: &mut Framebuffer, mouse_x: u32, mouse_y: u32) {
    let fb_width = framebuffer.width as usize;
    let offset = mouse_y as usize * fb_width + mouse_x as usize;
    let fb_raw = framebuffer.get_buffer_mut();

    let total_rows = 16.min(framebuffer.height as u32 - mouse_y) as usize;
    let total_cols = 16.min(framebuffer.width as u32 - mouse_x) as usize;
    for row in 0..total_rows {
        let row_offset = offset + row * fb_width;
        let mut cursor_row = CURSOR[row];
        let mut shadow = false;
        for col in 0..total_cols {
            if cursor_row & 1 != 0 {
                shadow = true;
                fb_raw[row_offset + col] = 0x0f;
            } else if shadow {
                shadow = false;
                fb_raw[row_offset + col] = 0x13;
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
