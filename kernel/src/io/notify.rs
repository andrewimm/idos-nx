use alloc::collections::BTreeSet;

pub struct NotifyQueue {
    io_listeners: BTreeSet<u32>,
    notification_ready: bool,
}

impl NotifyQueue {
    pub fn new() -> Self {
        Self {
            io_listeners: BTreeSet::new(),
            notification_ready: false,
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

    pub fn is_ready(&mut self) -> bool {
        let ready = self.notification_ready;
        self.notification_ready = false;
        ready
    }

    pub fn mark_ready(&mut self) {
        self.notification_ready = true;
    }
}
