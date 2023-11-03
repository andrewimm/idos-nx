use alloc::collections::BTreeSet;

pub struct NotifyQueue {
    io_listeners: BTreeSet<u32>,
}

impl NotifyQueue {
    pub fn new() -> Self {
        Self {
            io_listeners: BTreeSet::new(),
        }
    }

    pub fn add_listener(&mut self, index: u32) {
        self.io_listeners.insert(index);
    }

    pub fn contains(&self, index: u32) -> bool {
        self.io_listeners.contains(&index)
    }

    pub fn remove(&mut self, index: u32) {
        self.io_listeners.remove(&index);
    }
}
