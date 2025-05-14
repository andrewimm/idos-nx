use crate::memory::address::{PhysicalAddress, VirtualAddress};
use alloc::collections::BTreeSet;
use spin::RwLock;

/// Wake Set allows a Task to wait on multiple Futex addresses at the same time.
/// A Wake Set has multiple addresses associated with it. When a Task blocks on
/// the Set, each of those addresses is added to the Futex watch list.
/// When the Task is awakened, it resumes execution within the syscall that
/// blocked on the Wake Set. Before returning to userspace, the syscall cleans
/// up all of the Futex addresses that were added by the Wake Set.
pub struct WakeSet {
    watched_addresses: RwLock<BTreeSet<PhysicalAddress>>,
}

impl WakeSet {
    pub fn new() -> Self {
        Self {
            watched_addresses: RwLock::new(BTreeSet::new()),
        }
    }

    pub fn watch_address(&self, addr: VirtualAddress) {}
}
