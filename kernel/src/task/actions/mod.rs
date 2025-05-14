//! While the rest of the `task` module contains the logic for creating and
//! modifying Tasks, the `actions` submodule contains all of the high-level
//! actions that can be executed by the current task.

pub mod handle;
pub mod lifecycle;
pub mod memory;
pub mod sync;

use super::{id, messaging, switching};
pub use switching::yield_coop;

/// Pause the current task for a specified number of milliseconds. During that
/// time, it will not be run by the scheduler.
pub fn sleep(ms: u32) {
    let current_lock = switching::get_current_task();
    current_lock.write().sleep(ms);
    yield_coop();
}

/// Attempt to read an incoming Message from another task, blocking execution
/// until it arrives.
pub fn read_message_blocking(timeout: Option<u32>) -> (Option<messaging::MessagePacket>, bool) {
    let mut current_ticks = crate::time::system::get_system_ticks();
    let current_lock = switching::get_current_task();
    let (message, remaining) = current_lock
        .write()
        .read_message_blocking(current_ticks, timeout);
    if message.is_some() {
        return (message, remaining);
    }
    // no message, the task has switched to being blocked
    yield_coop();
    // on awake, either the timeout ended or a message was received
    current_ticks = crate::time::system::get_system_ticks();
    let queue_read_pair = current_lock.write().read_message(current_ticks);
    queue_read_pair
}

pub fn send_message(to_id: id::TaskID, message: messaging::Message, expiration: u32) {
    let current_id = switching::get_current_id();
    let current_ticks = crate::time::system::get_system_ticks();
    let recipient_lock = switching::get_task(to_id);
    if let Some(recipient) = recipient_lock {
        recipient
            .write()
            .receive_message(current_ticks, current_id, message, expiration);
    }
}
