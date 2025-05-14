use crate::{
    memory::address::{PhysicalAddress, VirtualAddress},
    task::paging::get_current_physical_address,
};
use alloc::{collections::BTreeSet, vec::Vec};

/// Wake Set allows a Task to wait on multiple Futex addresses at the same time.
/// A Wake Set has multiple addresses associated with it. When a Task blocks on
/// the Set, each of those addresses is added to the Futex watch list.
/// When the Task is awakened, it resumes execution within the syscall that
/// blocked on the Wake Set. Before returning to userspace, the syscall cleans
/// up all of the Futex addresses that were added by the Wake Set.
pub struct WakeSet {
    watched_addresses: BTreeSet<PhysicalAddress>,
}

impl WakeSet {
    pub fn new() -> Self {
        Self {
            watched_addresses: BTreeSet::new(),
        }
    }

    pub fn watch_address(&mut self, addr: VirtualAddress) {
        let paddr = match get_current_physical_address(addr) {
            Some(addr) => addr,
            None => return,
        };
        self.watched_addresses.insert(paddr);
    }

    pub fn get_addresses(&self) -> Vec<PhysicalAddress> {
        self.watched_addresses.iter().copied().collect()
    }
}
