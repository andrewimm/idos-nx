use alloc::collections::VecDeque;
use alloc::vec::Vec;
use idos_api::io::error::IOError;
use spin::RwLock;

use crate::arch::port::Port;
use crate::collections::SlotList;
use crate::io::driver::comms::DriverCommand;
use crate::io::filesystem::install_task_dev;
use crate::io::provider::IOResult;
use crate::memory::address::VirtualAddress;
use crate::memory::shared::release_buffer;
use crate::task::actions::io::driver_io_complete;
use crate::task::actions::memory::map_memory;
use crate::task::actions::yield_coop;
use crate::task::memory::MemoryBacking;
use crate::task::messaging::Message;
use crate::task::switching::get_current_id;
use crate::time::system::Timestamp;

use super::buffers::ConsoleBuffers;
use super::console::{
    textmode::{Color, ColorCode, TextCell},
    Console,
};
use super::graphics::font::Font;
use super::graphics::framebuffer::Framebuffer;
use super::input::{KeyAction, KeyState};

pub static IO_BUFFERS: RwLock<Vec<ConsoleBuffers>> = RwLock::new(Vec::new());

struct PendingRead {
    request_id: u32,
    buffer_start: *mut u8,
    max_length: usize,
}

impl PendingRead {
    pub fn complete(self, console_buffers: &ConsoleBuffers) -> usize {
        let input_buffer = &console_buffers.input_buffer;
        let write_buffer =
            unsafe { core::slice::from_raw_parts_mut(self.buffer_start, self.max_length) };
        let mut written = 0;
        while let Some(byte) = input_buffer.read() {
            write_buffer[written] = byte;
            written += 1;
            if written >= write_buffer.len() {
                break;
            }
        }

        release_buffer(
            VirtualAddress::new(self.buffer_start as u32),
            self.max_length,
        );
        driver_io_complete(self.request_id, Ok(written as u32));

        written
    }
}

const COLS: usize = 80;
const ROWS: usize = 25;

pub struct ConsoleManager {
    key_state: KeyState,
    text_buffer_base: VirtualAddress,
    current_time: Timestamp,

    current_console: usize,
    consoles: Vec<Console<COLS, ROWS>>,

    /// Mapping of open handles to the consoles they reference
    open_io: SlotList<usize>,
    pending_reads: SlotList<VecDeque<PendingRead>>,
}

impl ConsoleManager {
    pub fn new(text_buffer_base: VirtualAddress) -> Self {
        let mut consoles = Vec::with_capacity(1);

        Self {
            key_state: KeyState::new(),
            text_buffer_base,
            current_time: crate::time::system::get_system_time().to_timestamp(),

            current_console: 0,
            consoles,

            open_io: SlotList::new(),
            pending_reads: SlotList::new(),
        }
    }

    pub fn add_console(&mut self) -> usize {
        let new_console = Console::new();
        new_console.terminal.clear_buffer();
        self.consoles.push(new_console);
        let index = self.consoles.len() - 1;
        loop {
            if let Some(mut buffers) = IO_BUFFERS.try_write() {
                buffers.push(ConsoleBuffers::new());
                break;
            }
            yield_coop();
        }

        let name = alloc::format!("CON{}", index + 1);
        install_task_dev(&name, get_current_id(), index as u32);

        index
    }

    /// Take a key action from the keyboard interrupt handler and send it to the
    /// current console for processing. Depending on the key pressed and the
    /// mode of the console, it may trigger a flush. If any content is flushed
    /// to the IO buffers, it will also check for pending reads and copy bytes
    /// to them if available.
    pub fn handle_key_action(&mut self, action: KeyAction) {
        let mut input_bytes: [u8; 4] = [0; 4];
        let result = self.key_state.process_key_action(action, &mut input_bytes);
        if let Some(len) = result {
            // send input buffer to current console
            let console: &mut Console<80, 25> =
                self.consoles.get_mut(self.current_console).unwrap();
            let mut input = &input_bytes[..len];
            if console.send_input(input) {
                // if the console should flush, send input to the IO buffer.
                loop {
                    if let Some(mut buffers) = IO_BUFFERS.try_write() {
                        let console_buffers = buffers.get_mut(self.current_console).unwrap();
                        let input_buffer = &console_buffers.input_buffer;
                        let available = console.pending_input.len();
                        for input in console.pending_input.iter() {
                            input_buffer.write(*input);
                        }
                        console.pending_input.clear();

                        if available > 0 {
                            let pending_reads = self.pending_reads.get_mut(self.current_console);
                            if let Some(queue) = pending_reads {
                                while let Some(mut front) = queue.pop_front() {
                                    front.complete(console_buffers);
                                }
                            }
                        }
                        break;
                    }
                    yield_coop();
                }
            }
        }
    }

    pub fn handle_request(&mut self, message: &Message) -> Option<IOResult> {
        match DriverCommand::from_u32(message.message_type) {
            DriverCommand::OpenRaw => {
                let console_id = message.args[0] as usize;
                let handle = self.open_io.insert(console_id);
                Some(Ok(handle as u32))
            }
            DriverCommand::Close => {
                let instance = message.args[0];
                match self.open_io.remove(instance as usize) {
                    Some(_) => Some(Ok(1)),
                    None => Some(Err(IOError::FileHandleInvalid)),
                }
            }
            DriverCommand::Read => {
                let request_id = message.unique_id;
                let instance = message.args[0];
                let buffer_ptr = message.args[1] as *mut u8;
                let buffer_len = message.args[2] as usize;
                let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
                self.read(request_id, instance, buffer).inspect(|_| {
                    release_buffer(VirtualAddress::new(buffer_ptr as u32), buffer_len);
                })
            }
            DriverCommand::Write => {
                let instance = message.args[0];
                let buffer_ptr = message.args[1] as *const u8;
                let buffer_len = message.args[2] as usize;
                let buffer = unsafe { core::slice::from_raw_parts(buffer_ptr, buffer_len) };
                let result = self.write(instance, buffer);
                release_buffer(VirtualAddress::new(buffer_ptr as u32), buffer_len);
                Some(result)
            }
            _ => Some(Err(IOError::UnsupportedCommand)),
        }
    }

    pub fn read(&mut self, request_id: u32, instance: u32, buffer: &mut [u8]) -> Option<IOResult> {
        let console_id = match self.open_io.get(instance as usize) {
            Some(id) => id,
            None => return Some(Err(IOError::FileHandleInvalid)),
        };
        let mut bytes_written = 0;

        {
            if let Some(queue) = self.pending_reads.get_mut(*console_id) {
                if !queue.is_empty() {
                    // there are other pending reads, enqueue this one
                    let pending_read = PendingRead {
                        request_id,
                        buffer_start: buffer.as_mut_ptr(),
                        max_length: buffer.len(),
                    };
                    queue.push_back(pending_read);
                    return None;
                }
            }
        }

        let input_buffer = loop {
            if let Some(buffers) = IO_BUFFERS.try_read() {
                break buffers.get(*console_id).unwrap().input_buffer.clone();
            }
            yield_coop();
        };
        while bytes_written < buffer.len() {
            match input_buffer.read() {
                Some(ch) => {
                    buffer[bytes_written] = ch;
                    bytes_written += 1;
                }
                None => {
                    break;
                }
            }
        }
        if bytes_written < buffer.len() {
            let pending_read = PendingRead {
                request_id,
                buffer_start: buffer.as_mut_ptr(),
                max_length: buffer.len(),
            };
            match self.pending_reads.get_mut(*console_id) {
                Some(queue) => {
                    queue.push_back(pending_read);
                }
                None => {
                    let mut queue = VecDeque::new();
                    queue.push_back(pending_read);
                    self.pending_reads.replace(*console_id, queue);
                }
            }
            return None;
        }
        Some(Ok(bytes_written as u32))
    }

    pub fn write(&mut self, instance: u32, buffer: &[u8]) -> IOResult {
        let console_id = self
            .open_io
            .get(instance as usize)
            .ok_or(IOError::FileHandleInvalid)?;

        let console = self.consoles.get_mut(*console_id).unwrap();
        let mut i = 0;
        while i < buffer.len() {
            console.terminal.write_character(buffer[i]);
            i += 1;
        }
        Ok(buffer.len() as u32)
    }

    pub fn draw_window<F: Font>(&self, index: usize, fb: &mut Framebuffer, font: &F) {
        let console: &Console<80, 25> = self.consoles.get(index).unwrap();

        let window_x: u16 = 40;
        let window_y: u16 = 40;
        let inner_width: u16 = 640;
        let inner_height: u16 = 400;

        let framebuffer = fb.get_buffer_mut();

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

        font.draw_string(
            fb,
            window_x + 4,
            window_y + 24 + 4,
            "C:\\COMMAND.ELF".bytes(),
            0x00,
        );

        for row in 0..25 {
            font.draw_string(
                fb,
                window_x + 4,
                (window_y + 24 + 4) + (row as u16 * 16),
                console.row_text_iter(row),
                0x0f,
            );
        }
    }

    pub fn update_cursor(&self) {
        let cursor_offset = self
            .consoles
            .get(self.current_console)
            .unwrap()
            .terminal
            .get_cursor_offset();
        let register = Port::new(0x3d4);
        let register_value = Port::new(0x3d5);

        register.write_u8(0x0f);
        register_value.write_u8(cursor_offset as u8);
        register.write_u8(0x0e);
        register_value.write_u8((cursor_offset >> 8) as u8);
    }

    pub fn render_top_bar(&self) {
        let width = 80;
        let top_slice = unsafe {
            core::slice::from_raw_parts_mut(self.text_buffer_base.as_ptr_mut::<TextCell>(), width)
        };
        let title = " IDOS-NX ".as_bytes();
        for i in 0..title.len() {
            top_slice[i].glyph = title[i];
            top_slice[i].color = ColorCode::new(Color::White, Color::Blue);
        }
        for i in title.len()..width {
            top_slice[i] = TextCell {
                glyph: 0xcd,
                color: ColorCode::new(Color::White, Color::Black),
            };
        }

        //self.print_time();
    }

    pub fn update_clock<F: Font>(&mut self, fb: &mut Framebuffer, font: &F) {
        let current_time = crate::time::system::get_system_time().to_timestamp();
        if self.current_time != current_time {
            self.current_time = current_time;
        }
        self.print_time(fb, font);
        /*if self.current_time.total_minutes() != current_time.total_minutes() {
            self.current_time = current_time;
            self.print_time();
        }*/
    }

    pub fn print_time<F: Font>(&self, fb: &mut Framebuffer, font: &F) {
        let width = 80;
        let mut clock_buffer: [u8; 7] = [0x20; 7];
        self.current_time
            .to_datetime()
            .time
            .print_short_to_buffer(&mut clock_buffer[1..6]);
        /*
        let clock_color = ColorCode::new(Color::White, Color::Blue);
        let clock_start = width - clock_buffer.len();
        let top_slice = unsafe {
            core::slice::from_raw_parts_mut(
                self.text_buffer_base
                    .as_ptr_mut::<TextCell>()
                    .add(clock_start),
                clock_buffer.len(),
            )
        };
        for i in 0..clock_buffer.len() {
            top_slice[i] = TextCell {
                glyph: clock_buffer[i],
                color: clock_color,
            }
        }
        */
        let width = 7 * 8;
        font.draw_string(fb, 800 - width - 2, 4, clock_buffer.iter().cloned(), 0x0f);
    }

    pub fn clear_screen(&self) {
        let width = 80;
        let height = 25;

        for i in 0..(width * height) {
            unsafe {
                let ptr = self.text_buffer_base.as_ptr_mut::<TextCell>().add(i);
                *ptr = TextCell {
                    glyph: 0x20,
                    color: ColorCode::new(Color::LightGray, Color::Black),
                };
            }
        }
    }
}
