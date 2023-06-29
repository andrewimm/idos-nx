use core::cmp;
use core::sync::atomic::{AtomicU32, self};

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct TaskID(u32);

impl TaskID {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

impl cmp::Ord for TaskID {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for TaskID {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for TaskID {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for TaskID {}

impl Into<u32> for TaskID {
    fn into(self) -> u32 {
        self.0
    }
}

impl core::fmt::Debug for TaskID {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Task({})", self.0)
    }
}

impl core::fmt::Display for TaskID {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct IdGenerator(AtomicU32);

impl IdGenerator {
    pub const fn new() -> Self {
        Self(AtomicU32::new(1))
    }

    pub fn next(&self) -> TaskID {
        let id = self.0.fetch_add(1, atomic::Ordering::SeqCst);
        TaskID::new(id)
    }
}
