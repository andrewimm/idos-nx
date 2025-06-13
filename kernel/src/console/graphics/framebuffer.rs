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
                self.width as usize * self.height as usize,
            )
        }
    }
}
