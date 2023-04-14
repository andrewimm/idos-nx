use alloc::collections::VecDeque;
use super::id::TaskID;

/// Interprocess Messages are implemented by passing tuples of u32 values from
/// one task to another
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Message(pub u32, pub u32, pub u32, pub u32);

impl Message {
    pub fn empty() -> Self {
        Message(0, 0, 0, 0)
    }
}

/// A Message Packet associates a message with its sender
#[derive(Debug, Eq, PartialEq)]
pub struct MessagePacket {
    pub from: TaskID,
    pub message: Message,
}

impl MessagePacket {
    pub fn open(self) -> (TaskID, Message) {
        (self.from, self.message)
    }
}

/// For storing messages in a task's receiving queue, each message is
/// associated with an expiration time. The time is recorded in system ticks,
/// and indicates the time after which this entry is no longer valid.
/// Expiration is used to keep the queue from growing too large. Rather than
/// update all task queues whenever system time is increased, the kernel only
/// checks for expired items whenever the queue is accessed to add or remove
/// items.
pub struct EnqueuedMessage {
    pub packet: MessagePacket,
    pub expiration_ticks: u32,
}

/// Each task has a MessageQueue which stores messages that have been sent to
/// the task
pub struct MessageQueue {
    queue: VecDeque<EnqueuedMessage>,
}

impl MessageQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    fn remove_expired_items(&mut self, current_ticks: u32) {
        while let Some(entry) = self.queue.front() {
            if entry.expiration_ticks > current_ticks {
                return;
            }
            self.queue.pop_front();
        }
    }

    /// Add an incoming message from another task
    pub fn add(&mut self, from: TaskID, message: Message, current_ticks: u32, expiration_ticks: u32) {
        self.remove_expired_items(current_ticks);
        let for_queue = EnqueuedMessage {
            packet: MessagePacket {
                from,
                message,
            },
            expiration_ticks,
        };
        self.queue.push_back(for_queue);
    }

    /// Attempt to read a packet from the message queue. The first parameter of
    /// the return value is an option that may contain a packet if one exists.
    /// The second parameter is a boolean reflecting whether there are more
    /// packets to read.
    pub fn read(&mut self, current_ticks: u32) -> (Option<MessagePacket>, bool) {
        self.remove_expired_items(current_ticks);
        let message = self.queue.pop_front().map(|entry| entry.packet);
        let has_more = !self.queue.is_empty();
        (message, has_more)
    }

    /// Indicate whether there are any messages available for the task
    pub fn has_messages(&mut self, current_ticks: u32) -> bool {
        self.remove_expired_items(current_ticks);
        !self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use crate::task::id::TaskID;
    use super::{Message, MessagePacket, MessageQueue};

    #[test_case]
    fn add_and_read() {
        let mut queue = MessageQueue::new();
        {
            let (front, remaining) = queue.read(0);
            assert!(front.is_none());
            assert!(!remaining);
        }
        queue.add(
            TaskID::new(10),
            Message(1, 2, 3, 4),
            0,
            2000,
        );
        queue.add(
            TaskID::new(14),
            Message(5, 6, 7, 8),
            0,
            2000,
        );
        {
            let (front, remaining) = queue.read(0);
            assert_eq!(front.unwrap(), MessagePacket {
                from: TaskID::new(10),
                message: Message(1, 2, 3, 4),
            });
            assert!(remaining);
        }
        {
            let (front, remaining) = queue.read(0);
            assert_eq!(front.unwrap(), MessagePacket {
                from: TaskID::new(14),
                message: Message(5, 6, 7, 8),
            });
            assert!(!remaining);
        }
    }

    #[test_case]
    fn expiration() {
        let mut queue = MessageQueue::new();
        queue.add(
            TaskID::new(10),
            Message(1, 2, 3, 4),
            0,
            2000,
        );
        queue.add(
            TaskID::new(12),
            Message(5, 6, 7, 8),
            3000,
            5000,
        );
        {
            let (front, remaining) = queue.read(4000);
            assert_eq!(front.unwrap(), MessagePacket {
                from: TaskID::new(12),
                message: Message(5, 6, 7, 8),
            });
            assert!(!remaining);
        }
    }
}
