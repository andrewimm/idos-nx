//! Interprocess Communication (IPC) is achieved either by sending messages to
//! a Task's message queue, or by sharing memory buffers between tasks.

/// Interprocess Messages are implemented by passing these structures from one
/// task to another.
/// The message is composed of eight 32-bit fields. Canonically, the first two
/// fields are used to share the message type, as well as uniquely identify it
/// among other messages, making it easier to pair a responding message.
/// However all eight of the u32 fields can be used for any application-
/// specific purpose.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Message {
    pub message_type: u32,
    pub unique_id: u32,
    pub args: [u32; 6],
}

impl Message {
    pub fn empty() -> Self {
        Message {
            message_type: 0,
            unique_id: 0,
            args: [0; 6],
        }
    }

    pub fn set_args(mut self, args: [u32; 6]) -> Self {
        self.args = args;
        self
    }
}
