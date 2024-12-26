use idos_api::io::error::IOError;

use super::async_io::{
    AsyncOp, AsyncOpID, ASYNC_OP_CLOSE, ASYNC_OP_OPEN, ASYNC_OP_READ, ASYNC_OP_WRITE,
};

pub mod file;
pub mod irq;
pub mod message;
pub mod socket;
pub mod task;

pub type IOResult = Result<u32, IOError>;

pub trait IOProvider {
    /// Queue operations must be implemented by each Provider.
    /// enqueue_op adds a new op to be handled. Many providers implement a
    /// single operation queue, but some may have multiple parallel queues if,
    /// say, reads should not block writes (as is the case with sockets).
    /// The method returns a tuple. The first element is the unique ID of the
    /// enqueued Op, which can be used to reference, complete, or cancel the
    /// Op. The second field of the tuple is true if the Provider should
    /// run the first Op in the queue (usually if the enqueued Op is the only
    /// one)
    fn enqueue_op(&self, op: AsyncOp) -> (AsyncOpID, bool);

    fn peek_op(&self) -> Option<(AsyncOpID, AsyncOp)>;

    fn remove_op(&self, id: AsyncOpID) -> Option<AsyncOp>;

    /// Called when a task creates a new Op and submits it to the provider.
    fn op_request(&self, provider_index: u32, op: AsyncOp) -> Result<AsyncOpID, ()> {
        let (id, should_run) = self.enqueue_op(op);
        if should_run {
            self.run_next_op(provider_index);
        }
        Ok(id)
    }

    /// Run the first Op in the queue.
    fn run_next_op(&self, provider_index: u32) {
        let (id, op) = match self.peek_op() {
            Some((id, op)) => (id, op),
            None => return,
        };
        let immediate_result = match op.op_code & 0xfff {
            ASYNC_OP_OPEN => self.open(provider_index, id, op),
            ASYNC_OP_READ => self.read(provider_index, id, op),
            ASYNC_OP_WRITE => self.write(provider_index, id, op),
            ASYNC_OP_CLOSE => self.close(provider_index, id, op),
            _ => self.extended_op(provider_index, id, op),
        };
        match immediate_result {
            Some(res) => {
                self.complete_op(provider_index, id, res);
            }
            None => (),
        }
    }

    fn complete_op(&self, provider_index: u32, id: AsyncOpID, result: IOResult) {
        if let Some(op) = self.remove_op(id) {
            if op.op_code & 0xffff == ASYNC_OP_OPEN {
                // Opening a handle has some funky special behavior, since we
                // extract the driver lookup instance from the result, bind it
                // to this IO provider, and then overwrite the internal
                // details of that return value with a generic success state
                match result {
                    Ok(instance) => {
                        self.bind_to(instance);
                        op.complete(1);
                    }
                    Err(e) => op.complete_with_result(Err(e)),
                }
            } else {
                op.complete_with_result(result);
            }
        }
        self.run_next_op(provider_index);
    }

    fn bind_to(&self, instance: u32) {
        // default behavior is a no-op
    }

    fn open(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }

    fn read(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }

    fn write(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }

    fn close(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }

    fn extended_op(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }
}
