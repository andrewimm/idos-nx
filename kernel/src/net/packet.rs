pub trait PacketHeader: Sized {
    fn get_size() -> usize {
        core::mem::size_of::<Self>()
    }

    fn as_buffer(&self) -> &[u8] {
        let ptr = self as *const Self as *const u8;
        let size = <Self as PacketHeader>::get_size();
        unsafe {
            core::slice::from_raw_parts(ptr, size)
        }
    }

    fn copy_to_buffer(&self, buffer: &mut [u8]) -> usize {
        let size = Self::get_size();
        let location = buffer.len() - size;
        buffer[location..].copy_from_slice(self.as_buffer());
        location
    }

    fn from_buffer(buffer: &[u8]) -> Option<&Self> {
        let size = Self::get_size();
        if buffer.len() < size {
            return None;
        }
        unsafe { Some(&*(buffer.as_ptr() as *const Self)) }
    }
}

