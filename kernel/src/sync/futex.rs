//! Futex - Fast Userspace muTEX. Futex capabilities are provided by a few
//! syscall methods that provide atomic checking and sleeping of Tasks.

use crate::{
    memory::address::{PhysicalAddress, VirtualAddress},
    task::{
        actions::yield_coop,
        id::TaskID,
        paging::get_current_physical_address,
        switching::{get_current_id, get_current_task, get_task},
    },
};
use alloc::collections::{BTreeMap, VecDeque};
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

/// Stores the set of active Futex addresses, mapping them to all of the Tasks
/// that are currently waiting on them.
/// Because it is locked, all accesses *must* happen from interrupt-disabled
/// methods.
static FUTEX_WATCH_LIST: RwLock<BTreeMap<PhysicalAddress, VecDeque<TaskID>>> =
    RwLock::new(BTreeMap::new());

pub fn inject_watch_address(address: PhysicalAddress, task: TaskID) {
    let mut watch_list = FUTEX_WATCH_LIST.write();
    match watch_list.get_mut(&address) {
        Some(set) => set.push_back(task),
        None => {
            let mut set = VecDeque::new();
            set.push_back(task);
            watch_list.insert(address, set);
        }
    }
}

pub fn remove_watch_address(address: PhysicalAddress, task: TaskID) {
    let mut watch_list = FUTEX_WATCH_LIST.write();
    let remove_set = match watch_list.get_mut(&address) {
        Some(set) => {
            set.retain(|id| *id != task);
            set.is_empty()
        }
        None => return,
    };
    if remove_set {
        watch_list.remove(&address);
    }
}

/// Atomically checks if the value at `address` is still `value`. If it is,
/// the current Task waits until being woken by `futex_wake`.
/// In order for this to complete atomically, it must stop interrupts.
pub fn futex_wait(address: VirtualAddress, value: u32) {
    // TODO: disable interrupts; critical section.
    {
        futex_wait_inner(address, value);
    }
    yield_coop();
}

fn futex_wait_inner(address: VirtualAddress, value: u32) {
    let current_value = unsafe {
        let atomic = AtomicU32::from_ptr(address.as_ptr_mut::<u32>());
        atomic.load(Ordering::SeqCst)
    };
    if current_value != value {
        return;
    }
    let paddr = match get_current_physical_address(address) {
        Some(addr) => addr,
        None => return,
    };
    let current_task_id = get_current_id();

    {
        let mut watch_list = FUTEX_WATCH_LIST.write();
        match watch_list.get_mut(&paddr) {
            Some(set) => set.push_back(current_task_id),
            None => {
                let mut set = VecDeque::new();
                set.push_back(current_task_id);
                watch_list.insert(paddr, set);
            }
        }
    }

    // All accesses to this structure must be in critical sections to avoid
    // deadlocks. Maybe we can clean this up in the future.
    get_current_task().write().futex_wait(None);
}

/// Wakes up to `count` number of Tasks that may be blocked by previous calls to
/// `futex_wait` on the specific Physical Address backing `address`.
pub fn futex_wake(address: VirtualAddress, count: u32) {
    {
        futex_wake_inner(address, count);
    }
}

fn futex_wake_inner(address: VirtualAddress, count: u32) {
    let paddr = match get_current_physical_address(address) {
        Some(addr) => addr,
        None => return,
    };
    if count == 0 {
        return;
    }
    {
        let mut watch_list = FUTEX_WATCH_LIST.write();
        let remove_address = match watch_list.get_mut(&paddr) {
            Some(set) => {
                let mut to_wake = count;
                while to_wake > 0 && !set.is_empty() {
                    if let Some(wake_id) = set.pop_front() {
                        if let Some(task) = get_task(wake_id) {
                            task.write().futex_wake();
                        }
                    }
                    to_wake -= 1;
                }
                // remove the set entirely if it's now empty
                set.is_empty()
            }
            None => return,
        };

        if remove_address {
            watch_list.remove(&paddr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{futex_wait, futex_wake};
    use crate::memory::address::VirtualAddress;
    use crate::task::actions::handle::{
        create_kernel_task, handle_op_read_struct, open_message_queue,
    };
    use crate::task::actions::lifecycle::terminate;
    use crate::task::actions::send_message;
    use crate::task::messaging::Message;
    use alloc::boxed::Box;
    use core::sync::atomic::AtomicU32;

    #[test_case]
    fn simple_futex() {
        let futex = Box::new(AtomicU32::new(1));

        // child task receives the virtual address in a message, and
        // wakes it
        fn waker_task() -> ! {
            let messages = open_message_queue();
            let mut message = Message::empty();
            let read_op = handle_op_read_struct(messages, &mut message);
            read_op.wait_for_completion();

            let vaddr = message.args[0];
            futex_wake(VirtualAddress::new(vaddr), 1);
            terminate(1);
        }
        let (_, child_id) = create_kernel_task(waker_task, Some("WAKER"));
        let mut message = Message::empty();
        message.args[0] = futex.as_ptr() as u32;
        send_message(child_id, message, 0xffffffff);

        futex_wait(VirtualAddress::new(futex.as_ptr() as u32), 1);
    }
}
