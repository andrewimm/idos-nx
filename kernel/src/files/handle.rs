

/// A DriverHandle is a unique identifier used by a filesystem driver to track
/// an open file
#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct DriverHandle(pub u32);

impl Into<u32> for DriverHandle {
    fn into(self) -> u32 {
        self.0
    }
}

impl Into<usize> for DriverHandle {
    fn into(self) -> usize {
        self.0 as usize
    }
}

