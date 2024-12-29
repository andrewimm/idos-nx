pub trait PacketHeader: Sized {
    fn get_size() -> usize {
        core::mem::size_of::<Self>()
    }

    fn as_u8_buffer(&self) -> &[u8] {
        let ptr = self as *const Self as *const u8;
        let size = <Self as PacketHeader>::get_size();
        unsafe { core::slice::from_raw_parts(ptr, size) }
    }

    fn try_as_u16_buffer(&self) -> Option<&[u16]> {
        let original_size = <Self as PacketHeader>::get_size();
        if original_size % 2 != 0 {
            return None;
        }
        let ptr = self as *const Self as *const u16;
        unsafe { Some(core::slice::from_raw_parts(ptr, original_size / 2)) }
    }

    /// Copies the header to the end of the provided u8 slice
    fn copy_to_u8_buffer(&self, buffer: &mut [u8]) -> usize {
        let size = Self::get_size();
        let location = buffer.len() - size;
        buffer[location..].copy_from_slice(self.as_u8_buffer());
        location
    }

    fn try_from_u8_buffer(buffer: &[u8]) -> Option<&Self> {
        let size = Self::get_size();
        if buffer.len() < size {
            return None;
        }
        unsafe { Some(&*(buffer.as_ptr() as *const Self)) }
    }
}
