use crate::memory::address::VirtualAddress;

pub struct Console {
    cursor_x: u8,
    cursor_y: u8,

    width: u8,
    height: u8,

    text_buffer_base: VirtualAddress,
}

impl Console {
    pub fn new(text_buffer_base: VirtualAddress) -> Self {
        Self {
            cursor_x: 0,
            cursor_y: 24,

            width: 80,
            height: 25,

            text_buffer_base,
        }
    }
    
    pub fn get_text_buffer_base_ptr(&self) -> *mut TextCell {
        self.text_buffer_base.as_ptr_mut::<TextCell>()
    }

    pub fn get_text_buffer(&self) -> &mut [TextCell] {
        unsafe {
            let ptr = self.get_text_buffer_base_ptr();
            let len = 80 * 25;
            core::slice::from_raw_parts_mut(ptr, len)
        }
    }

    pub fn send_input(&mut self, input: &[u8]) {
        for ch in input {
            // these are actually supposed to go to the Console device file,
            // but just print it for now
            self.write_character(*ch);
        }
    }

    pub fn write_character(&mut self, ch: u8) {
        if ch < 0x20 {
            // non printable
        } else {
            self.put_raw_character(ch);
        }
    }

    pub fn put_raw_character(&mut self, ch: u8) {
        let index = self.cursor_y as usize *
            self.width as usize +
            self.cursor_x as usize;
        self.get_text_buffer()[index].glyph = ch;

        self.advance_cursor();
    }

    pub fn advance_cursor(&mut self) {
        self.cursor_x += 1;
        if self.cursor_x >= self.width {
            self.cursor_x -= self.width;
            self.newline();
        }
    }

    pub fn newline(&mut self) {
        self.cursor_y += 1;
        if self.cursor_y >= self.height {
            self.cursor_y = self.height - 1;
        }
        self.scroll(1);
    }

    pub fn scroll(&mut self, scroll_delta: u8) {
        if scroll_delta == 0 {
            return;
        }
        if scroll_delta >= self.height {
            //self.clear_screen();
            return;
        }
        let rows_to_scroll = self.height - scroll_delta;
        for row in 0..rows_to_scroll {
            let to = unsafe {
                let to_ptr = self.get_text_buffer_base_ptr().add(self.width as usize * row as usize);
                core::slice::from_raw_parts_mut(to_ptr, self.width as usize)
            };
            let from = unsafe {
                let from_ptr = self.get_text_buffer_base_ptr().add(self.width as usize * (row + scroll_delta) as usize);
                core::slice::from_raw_parts(from_ptr, self.width as usize)
            };
            to.copy_from_slice(from);
        }
        let remainder_start = self.width as usize * rows_to_scroll as usize;
        let remainder_buffer = &mut self.get_text_buffer()[remainder_start..];
        for cell in remainder_buffer {
            cell.glyph = 0x20;
        }
    }
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct TextCell {
    glyph: u8,
    color: ColorCode,
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct ColorCode(pub u8);
