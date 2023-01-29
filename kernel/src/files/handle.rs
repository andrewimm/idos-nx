

/// A DriverHandle is a unique identifier used by a filesystem driver to track
/// an open file
#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct DriverHandle(u32);


