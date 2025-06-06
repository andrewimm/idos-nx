use core::sync::atomic::{AtomicU32, Ordering};

use alloc::vec::Vec;
use spin::RwLock;

use crate::conman::{register_console_manager, InputBuffer};
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::sync::futex::{futex_wait, futex_wake};
use crate::task::actions::lifecycle::{create_kernel_task, terminate};
use crate::task::actions::memory::map_memory;
use crate::task::actions::sync::{block_on_wake_set, create_wake_set};
use crate::task::id::TaskID;
use crate::task::memory::MemoryBacking;

use self::input::KeyAction;
use self::manager::ConsoleManager;

pub mod buffers;
pub mod console;
pub mod driver;
pub mod input;
pub mod manager;

pub static IO_BUFFERS: RwLock<Vec<buffers::ConsoleBuffers>> = RwLock::new(Vec::new());

pub fn manager_task() -> ! {
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

    conman.clear_screen();
    conman.render_top_bar();

    let mut last_action_type: u8 = 0;
    loop {
        // read input actions and pass them to the current console for state
        // management
        loop {
            let next_action = match keyboard_buffer.read() {
                Some(action) => action,
                None => break,
            };
            if last_action_type == 0 {
                last_action_type = next_action;
            } else {
                match KeyAction::from_raw(last_action_type, next_action) {
                    Some(action) => conman.handle_action(action),
                    None => (),
                }
                last_action_type = 0;
            }
        }

        // read pending bytes for each console, and process them
        conman.process_buffers();

        conman.update_cursor();

        conman.update_clock();

        block_on_wake_set(wake_set, Some(1000));
    }
}

pub fn init_console() {
    create_kernel_task(manager_task, Some("CONMAN"));
}

pub fn console_ready() {
    crate::command::start_command(0);
}
