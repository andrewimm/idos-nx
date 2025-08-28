use crate::memory::address::VirtualAddress;

pub struct Framebuffer {
    pub width: u16,
    pub height: u16,
    pub stride: u16,

    pub buffer: VirtualAddress,
}

impl Framebuffer {
    pub fn get_buffer_mut(&self) -> &'static mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.buffer.as_ptr_mut::<u8>(),
                self.stride as usize * self.height as usize,
            )
        }
    }

    /// Return a smaller FrameBuffer that only contains the contents up until
    /// the specified row
    pub fn before_row(&self, row: u16) -> Self {
        // width and stride should be the same, height is `row`
        Self {
            width: self.width,
            height: row,
            stride: self.stride,
            buffer: self.buffer,
        }
    }

    /// Return a smaller FrameBuffer that only contains the contents from the
    /// specified row up until the end
    pub fn from_row(&self, row: u16) -> Self {
        // width and stride are the same, height is previous height - row
        let offset = (self.stride as u32) * (row as u32);
        Self {
            width: self.width,
            stride: self.stride,
            height: self.height - row,
            buffer: self.buffer + offset,
        }
    }

    pub fn from_offset(&self, col: u16, row: u16) -> Self {
        let offset = (self.stride as u32) * (row as u32) + (col as u32);
        Self {
            width: self.width - col,
            stride: self.stride,
            height: self.height - row,
            buffer: self.buffer + offset,
        }
    }
}
