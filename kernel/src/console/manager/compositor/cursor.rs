pub struct Cursor {
    pub width: u16,
    pub height: u16,
    pub bitmap: &'static [u16],
}

impl Cursor {
    pub fn new(width: u16, height: u16, bitmap: &'static [u16]) -> Self {
        Self {
            width,
            height,
            bitmap,
        }
    }
}
