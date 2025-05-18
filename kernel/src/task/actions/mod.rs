//! While the rest of the `task` module contains the logic for creating and
//! modifying Tasks, the `actions` submodule contains all of the high-level
//! actions that can be executed by the current task.

pub mod handle;
pub mod io;
pub mod lifecycle;
pub mod memory;
pub mod sync;

use crate::io::async_io::IOType;

use super::{id, messaging, switching};
pub use switching::yield_coop;

/// Pause the current task for a specified number of milliseconds. During that
/// time, it will not be run by the scheduler.
pub fn sleep(ms: u32) {
    let current_lock = switching::get_current_task();
    current_lock.write().sleep(ms);
    yield_coop();
}

pub fn send_message(to_id: id::TaskID, message: messaging::Message, expiration: u32) {
    let current_id = switching::get_current_id();
    let current_ticks = crate::time::system::get_system_ticks();
    let recipient_lock = switching::get_task(to_id);
    if let Some(recipient) = recipient_lock {
        recipient
            .write()
            .receive_message(current_ticks, current_id, message, expiration);

        let message_provider = recipient.read().get_message_io_provider().clone();
        if let Some((_io_index, message_io)) = message_provider {
            match *message_io {
                IOType::MessageQueue(ref io_provider) => {
                    io_provider.check_messages();
                }
                _ => (),
            }
        }
    }
}
