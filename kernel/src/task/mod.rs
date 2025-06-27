use crate::log::TaggedLogger;

pub mod actions;
pub mod args;
pub mod id;
pub mod map;
pub mod memory;
pub mod messaging;
pub mod paging;
pub mod scheduling;
pub mod stack;
pub mod state;
pub mod switching;

const LOGGER: TaggedLogger = TaggedLogger::new("TASK", 35);

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
        fn wait_for_child_inner() -> ! {
            super::actions::lifecycle::terminate(4);
        }
        let (child_handle, _child_task) =
            super::actions::handle::create_kernel_task(wait_for_child_inner, Some("CHILD"));
        let result = super::actions::io::read_sync(child_handle, &mut [], 0);
        assert_eq!(result, Ok(4));
    }
}
