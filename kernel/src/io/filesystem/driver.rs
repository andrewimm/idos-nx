pub struct DriverID(usize);

impl DriverID {
    pub fn new(index: usize) -> Self {
        Self(index)
    }
}

impl core::ops::Deref for DriverID {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub enum DriverType {
    SyncDevice,
    AsyncDevice,
    SyncFilesystem,
    AsyncFilesystem,
}
