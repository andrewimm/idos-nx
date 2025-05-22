#[repr(transparent)]
pub struct Handle(u32);

impl Handle {
    pub fn new(handle: u32) -> Self {
        Handle(handle)
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}
