use super::async_io::{AsyncOp, AsyncOpID};

pub mod file;
pub mod irq;
pub mod message;
pub mod task;

pub trait IOProvider {
    fn add_op(&mut self, index: u32, op: AsyncOp) -> Result<AsyncOpID, ()>;
}
