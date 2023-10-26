use super::async_io::AsyncOp;

pub mod message;
pub mod task;

pub trait IOProvider {
    fn add_op(&mut self, op: AsyncOp) -> Result<(), ()>;
}
