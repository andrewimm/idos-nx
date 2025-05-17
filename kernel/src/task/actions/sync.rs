use super::yield_coop;
use crate::{
    io::handle::Handle,
    memory::address::VirtualAddress,
    sync::{
        futex::{inject_watch_address, remove_watch_address},
        wake_set::WakeSet,
    },
    task::switching::get_current_task,
};

pub fn create_wake_set() -> Handle {
    let wake_set = WakeSet::new();
    let current_task_lock = get_current_task();
    let mut current_task_guard = current_task_lock.write();
    current_task_guard.wake_sets.insert(wake_set)
}

pub fn add_address_to_wake_set(set_id: Handle, address: VirtualAddress) {
    let current_task_lock = get_current_task();
    let mut current_task_guard = current_task_lock.write();
    match current_task_guard.wake_sets.get_mut(set_id) {
        Some(set) => set.watch_address(address),
        None => (),
    }
}

pub fn block_on_wake_set(set_id: Handle, timeout: Option<u32>) {
    let (current_id, addresses) = {
        let current_task_lock = get_current_task();
        let mut current_task_guard = current_task_lock.write();
        let addresses = match current_task_guard.wake_sets.get(set_id) {
            Some(set) => set.get_addresses(),
            None => return,
        };
        let id = current_task_guard.id;
        for address in addresses.iter() {
            inject_watch_address(*address, id);
        }
        current_task_guard.futex_wait(timeout);

        (id, addresses)
    };
    // Task is marked as waiting on a futex, with multiple addresses injected to
    // possibly wake it. Yielding means we won't return to this method until it
    // is awakened.
    yield_coop();
    // By now, the task was awakened. Clean up all of the injected addresses
    for address in addresses {
        remove_watch_address(address, current_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{add_address_to_wake_set, block_on_wake_set, create_wake_set};
    use crate::memory::address::VirtualAddress;
    use crate::sync::futex::futex_wake;
    use crate::task::actions::handle::{
        create_kernel_task, handle_op_read_struct, open_message_queue,
    };
    use crate::task::actions::io::read_struct_sync;
    use crate::task::actions::lifecycle::terminate;
    use crate::task::actions::send_message;
    use crate::task::messaging::Message;
    use alloc::boxed::Box;
    use core::sync::atomic::AtomicU32;

    #[test_case]
    fn wake_address_in_set() {
        let wake_set = create_wake_set();

        let futex = Box::new(AtomicU32::new(1));
        let futex_2 = Box::new(AtomicU32::new(0));
        let futex_3 = Box::new(AtomicU32::new(0));

        add_address_to_wake_set(wake_set, VirtualAddress::new(futex.as_ptr() as u32));
        add_address_to_wake_set(wake_set, VirtualAddress::new(futex_2.as_ptr() as u32));
        add_address_to_wake_set(wake_set, VirtualAddress::new(futex_3.as_ptr() as u32));

        fn waker_task() -> ! {
            let messages = open_message_queue();
            let mut message = Message::empty();
            read_struct_sync(messages, &mut message);

            let vaddr = message.args[0];
            futex_wake(VirtualAddress::new(vaddr), 1);
            terminate(1);
        }
        let (_, child_id) = create_kernel_task(waker_task, Some("WAKER"));
        let mut message = Message::empty();
        message.args[0] = futex.as_ptr() as u32;
        send_message(child_id, message, 0xffffffff);

        block_on_wake_set(wake_set, None);
    }
}
