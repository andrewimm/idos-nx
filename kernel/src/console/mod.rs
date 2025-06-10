use core::sync::atomic::{AtomicU32, Ordering};

use alloc::vec::Vec;
use idos_api::io::AsyncOp;
use spin::RwLock;

use crate::conman::{register_console_manager, InputBuffer};
use crate::io::async_io::ASYNC_OP_READ;
use crate::io::handle::Handle;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::sync::futex::{futex_wait, futex_wake};
use crate::task::actions::handle::{create_pipe_handles, open_message_queue, transfer_handle};
use crate::task::actions::io::{
    append_io_op, close_sync, driver_io_complete, read_sync, write_sync,
};
use crate::task::actions::lifecycle::{create_kernel_task, terminate};
use crate::task::actions::memory::map_memory;
use crate::task::actions::sync::{block_on_wake_set, create_wake_set};
use crate::task::id::TaskID;
use crate::task::memory::MemoryBacking;
use crate::task::messaging::Message;

use self::input::KeyAction;
use self::manager::ConsoleManager;

pub mod buffers;
pub mod console;
//pub mod driver;
pub mod input;
pub mod manager;

pub fn manager_task() -> ! {
    let response_writer = Handle::new(0);

    let wake_set = create_wake_set();

    let input_buffer_addr = match register_console_manager(wake_set) {
        Ok(addr) => addr,
        Err(_) => {
            crate::kprintln!("Failed to register CONMAN");
            terminate(0);
        }
    };

    let keyboard_buffer_ptr =
        input_buffer_addr.as_ptr::<InputBuffer<{ crate::conman::INPUT_BUFFER_SIZE }>>();
    let keyboard_buffer = unsafe { &*keyboard_buffer_ptr };

    let text_buffer_base = map_memory(
        None,
        0x1000,
        MemoryBacking::Direct(PhysicalAddress::new(0xb8000)),
    )
    .unwrap();

    let mut conman = ConsoleManager::new(text_buffer_base);
    conman.add_console(); // create the first console (CON1)

    conman.clear_screen();
    conman.render_top_bar();

    let _ = write_sync(response_writer, &[0], 0);
    let _ = close_sync(response_writer);

    let messages_handle = open_message_queue();
    let mut incoming_message = Message::empty();

    let mut message_read = AsyncOp::new(
        ASYNC_OP_READ,
        &mut incoming_message as *mut Message as u32,
        core::mem::size_of::<Message>() as u32,
        0,
    );
    let _ = append_io_op(messages_handle, &message_read, Some(wake_set));

    let mut last_action_type: u8 = 0;
    loop {
        loop {
            // read input actions and pass them to the current console
            let next_action = match keyboard_buffer.read() {
                Some(action) => action,
                None => break,
            };
            if last_action_type == 0 {
                last_action_type = next_action;
            } else {
                match KeyAction::from_raw(last_action_type, next_action) {
                    Some(action) => {
                        conman.handle_key_action(action);
                    }
                    None => (),
                }
                last_action_type = 0;
            }
        }

        if message_read.is_complete() {
            let sender = message_read.return_value.load(Ordering::SeqCst);
            let request_id = incoming_message.unique_id;
            match conman.handle_request(&incoming_message) {
                Some(result) => driver_io_complete(request_id, result),
                None => (),
            }

            message_read = AsyncOp::new(
                ASYNC_OP_READ,
                &mut incoming_message as *mut Message as u32,
                core::mem::size_of::<Message>() as u32,
                0,
            );
            let _ = append_io_op(messages_handle, &message_read, Some(wake_set));
        }

        conman.update_cursor();
        conman.update_clock();

        block_on_wake_set(wake_set, Some(1000));
    }
}

pub fn init_console() {
    let (response_reader, response_writer) = create_pipe_handles();
    let driver_task = create_kernel_task(manager_task, Some("CONMAN"));
    transfer_handle(response_writer, driver_task);

    let _ = read_sync(response_reader, &mut [0u8], 0);
    let _ = close_sync(response_reader);
}

pub fn console_ready() {
    crate::command::start_command(0);
}
