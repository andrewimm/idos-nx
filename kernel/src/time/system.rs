//! Utilities for managing system time

use core::sync::atomic::{AtomicU32, Ordering};

// The system timer ticks at ~100Hz
pub const HUNDRED_NS_PER_TICK: u64 = 100002;
pub const MS_PER_TICK: u32 = (HUNDRED_NS_PER_TICK / 10000) as u32;

/// Stores the number of clock ticks since the kernel began execution. This is
/// used for relative time offsets within various kernel internals.
static SYSTEM_TICKS: AtomicU32 = AtomicU32::new(0);

pub fn tick() {
    let old: u32 = SYSTEM_TICKS.fetch_add(1, Ordering::SeqCst);
    let old_ms = old * MS_PER_TICK;
    let new_ms = old_ms + MS_PER_TICK;
    if new_ms / 1000 > old_ms / 1000 {
        crate::kprint!("TICK\n");
    }
}
