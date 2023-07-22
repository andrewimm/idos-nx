pub trait FileHeader where Self: Sized {
    fn as_buffer_mut(&mut self) -> &mut [u8] {
        let len = core::mem::size_of::<Self>();
        let ptr = self as *mut Self as *mut u8;
        unsafe {
            core::slice::from_raw_parts_mut(ptr, len)
        }
    }
}
