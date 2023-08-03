use core::ops::Deref;

#[derive(Copy, Clone)]
pub struct FileHandle(pub u32);

impl Deref for FileHandle {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::fmt::Write for FileHandle {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        crate::syscall::io::write_str(*self, s);
        core::fmt::Result::Ok(())
    }
}
