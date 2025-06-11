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
