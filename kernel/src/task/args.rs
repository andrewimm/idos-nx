//! Arguments passed to new executable tasks
//!
//! When a task is created, it begin in an empty state with no runnable code.
//! Only after a program has been loaded and attached is it possible to run
//! the task. During this intermediate period, it is also possible to pass a
//! series of arguments to the task.
//! The `ExecArgs` struct on a Task object uses the builder model. Args can
//! be added incrementally, and when the task begins to execute the final state
//! is compiled. These values are placed on the initial stack of the task, where
//! they can be read by a program, similar to the `argv` array in POSIX/C.

use alloc::vec::Vec;

/// Args are treated externally as a set of distinct strings. Internally we
/// store them as a continuous array of bytes, with individual lengths marking
/// the barrier between arguments. This is the format that will be copied to
/// the task stack when it first executes.
pub struct ExecArgs {
    raw: Vec<u8>,
    lengths: Vec<u32>,
}

impl ExecArgs {
    pub fn new() -> Self {
        Self {
            raw: Vec::new(),
            lengths: Vec::new(),
        }
    }

    /// Append a new argument to the list, storing it in a way that will be
    /// easily copied to the stack later.
    pub fn add(&mut self, arg: &str) -> &mut Self {
        self.lengths.push(arg.len() as u32 + 1);
        for b in arg.bytes() {
            self.raw.push(b);
        }
        self.raw.push(0);
        self
    }

    pub fn arg_string(&self) -> &Vec<u8> {
        &self.raw
    }

    pub fn arg_lengths(&self) -> &Vec<u32> {
        &self.lengths
    }

    pub fn arg_count(&self) -> u32 {
        self.lengths.len() as u32
    }

    /// Determine how much stack space is required to copy the argument
    /// information to the task's stack.
    /// The actual argument bytes will be zero-padded to a 4-byte boundary.
    /// The lengths will be copied as a series of 32-bit integers, and the total
    /// count will also be a 32-bit integer.
    pub fn stack_size(&self) -> usize {
        let mut string_length = self.raw.len();
        if string_length & 3 != 0 {
            string_length += 4 - (string_length & 3);
        }
        string_length + self.lengths.len() * 4 + 4
    }
}
