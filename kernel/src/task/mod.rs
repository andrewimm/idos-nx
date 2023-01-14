pub mod id;
pub mod messaging;
pub mod stack;
pub mod state;
pub mod switching;

pub use switching::yield_coop;

pub fn sleep(ms: u32) {
    let current_lock = switching::get_current_task();
    current_lock.write().sleep(ms);
    yield_coop();
}
