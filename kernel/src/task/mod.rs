pub mod actions;
pub mod files;
pub mod id;
pub mod memory;
pub mod messaging;
pub mod paging;
pub mod registers;
pub mod stack;
pub mod state;
pub mod switching;

#[cfg(test)]
mod tests {

    #[test_case]
    fn switching_works() {
        crate::kprint!("\n");
        super::actions::yield_coop();
    }

    #[test_case]
    fn wake_from_sleep() {
        super::actions::sleep(1);
    }

    #[test_case]
    fn wait_for_child() {
        let child_task = super::actions::lifecycle::create_kernel_task(wait_for_child_inner);
        let result = super::actions::lifecycle::wait_for_child(child_task, None);
        assert_eq!(result, 4);
    }

    fn wait_for_child_inner() -> ! {
        super::actions::lifecycle::terminate(4);
    }

    #[test_case]
    fn message_passing() {
        let child_task = super::actions::lifecycle::create_kernel_task(message_passing_inner);
        super::actions::send_message(child_task, super::messaging::Message(1, 2, 3, 4), 0xffffffff);
        super::actions::lifecycle::wait_for_child(child_task, None);
    }

    fn message_passing_inner() -> ! {
        let (packet, _) = super::actions::read_message_blocking(None);
        let (_, message) = packet.unwrap().open();
        assert_eq!(message, super::messaging::Message(1, 2, 3, 4));
        super::actions::lifecycle::terminate(0);
    }
}
