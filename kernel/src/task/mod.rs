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
    use crate::memory::physical::with_allocator;

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

    #[test_case]
    fn sharing_memory() {
        use super::actions::handle::{create_kernel_task, open_message_queue};
        use super::actions::io::read_struct_sync;
        use super::actions::lifecycle::terminate;
        use super::actions::memory::{map_memory, unmap_memory};
        use super::actions::{send_message, yield_coop};
        use super::memory::MemoryBacking;
        use super::messaging::Message;
        use super::paging::get_current_physical_address;
        use crate::memory::address::{PhysicalAddress, VirtualAddress};
        use crate::memory::physical::tracked_frame_reference_count;

        // allocate memory in one task, share it with another task, and verify
        // that it is properly tracked by the memory manager

        fn child_task() -> ! {
            let mut message = Message::empty();
            let message_queue = open_message_queue();
            let _ = read_struct_sync(message_queue, &mut message, 0).unwrap();
            let paddr = PhysicalAddress::new(message.args[0]);
            // map it directly
            let mapped_addr = map_memory(None, 0x1000, MemoryBacking::Direct(paddr)).unwrap();

            // direct mapping is immediately tracked
            assert_eq!(tracked_frame_reference_count(paddr), Some(2));

            let buffer =
                unsafe { core::slice::from_raw_parts_mut(mapped_addr.as_u32() as *mut u8, 0x1000) };
            assert_eq!(buffer[0], 0xaa);
            buffer[0] = 0xbb;

            unmap_memory(mapped_addr, 0x1000).unwrap();

            // after unmapping, the tracking count should decrement
            assert_eq!(tracked_frame_reference_count(paddr), Some(1));

            terminate(0);
        }

        let (child_handle, child_task) = create_kernel_task(child_task, Some("CHILD"));
        // map memory here, and then share it with the child task
        let mapped_to = map_memory(None, 0x1000, MemoryBacking::FreeMemory).unwrap();
        let local_buffer =
            unsafe { core::slice::from_raw_parts_mut(mapped_to.as_u32() as *mut u8, 0x1000) };

        // writing will force the page to be allocated
        local_buffer[0] = 0xaa;

        // send the physical address to the child task
        let paddr = get_current_physical_address(mapped_to).unwrap();
        send_message(
            child_task,
            Message {
                message_type: 0,
                unique_id: 0,
                args: [paddr.as_u32(), 0, 0, 0, 0, 0],
            },
            0xffffffff,
        );

        // wait for the child to write to the page
        while local_buffer[0] != 0xbb {
            yield_coop();
        }

        let _ = super::actions::io::read_sync(child_handle, &mut [], 0);
        unmap_memory(mapped_to, 0x1000).unwrap();
        // tracking count is zero, and frame is now freed
        assert!(tracked_frame_reference_count(paddr).is_none());
        with_allocator(|alloc| {
            assert!(!alloc.is_address_allocated(paddr));
        });
    }
}
