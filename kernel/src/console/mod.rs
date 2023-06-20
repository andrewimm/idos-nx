use core::sync::atomic::{AtomicU32, Ordering};

use crate::hardware::ps2::{keyboard::KeyAction, keycodes::KeyCode};
use crate::collections::RingBuffer;
use crate::memory::address::PhysicalAddress;
use crate::task::actions::lifecycle::wait_for_io;
use crate::task::actions::memory::map_memory;
use crate::task::id::TaskID;
use crate::task::memory::MemoryBacking;
use crate::task::switching::get_task;

use self::manager::ConsoleManager;

pub mod console;
pub mod input;
pub mod manager;

static INPUT_BUFFER_RAW: [KeyAction; 32] = [KeyAction::Release(KeyCode::None); 32];
pub static INPUT_BUFFER: RingBuffer<KeyAction> = RingBuffer::for_buffer(&INPUT_BUFFER_RAW);

static CONSOLE_MANAGER_TASK: AtomicU32 = AtomicU32::new(0);

pub fn register_console_manager(id: TaskID) {
    CONSOLE_MANAGER_TASK.store(id.into(), Ordering::SeqCst);
}

pub fn get_console_manager_id() -> TaskID {
    TaskID::new(
        CONSOLE_MANAGER_TASK.load(Ordering::SeqCst)
    )
}

pub fn wake_console_manager() {
    let id = get_console_manager_id();
    if let Some(lock) = get_task(id) {
        if let Some(mut task) = lock.try_write() {
            task.io_complete();
        }
    }
}

pub fn manager_task() -> ! {
    let text_buffer_base = map_memory(None, 0x1000, MemoryBacking::Direct(PhysicalAddress::new(0xb8000))).unwrap();

    let mut conman = ConsoleManager::new(text_buffer_base);

    loop {
        // read input actions and pass them to the current console for state
        // management
        loop {
            let action = match INPUT_BUFFER.read() {
                Some(action) => action,
                None => break,
            };
            conman.handle_action(action);
        }

        wait_for_io(Some(1000));
    }
}
