use core::sync::atomic::{AtomicU32, Ordering};

use alloc::vec::Vec;
use spin::RwLock;

use crate::collections::RingBuffer;
use crate::hardware::ps2::{keyboard::KeyAction, keycodes::KeyCode};
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::sync::futex::{futex_wait, futex_wake};
use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::actions::memory::map_memory;
use crate::task::id::TaskID;
use crate::task::memory::MemoryBacking;
use crate::task::switching::get_task;

use self::manager::ConsoleManager;

pub mod buffers;
pub mod console;
pub mod driver;
pub mod input;
pub mod manager;

static INPUT_BUFFER_RAW: [KeyAction; 32] = [KeyAction::Release(KeyCode::None); 32];
pub static INPUT_BUFFER: RingBuffer<KeyAction> = RingBuffer::for_buffer(&INPUT_BUFFER_RAW);

static CONSOLE_MANAGER_TASK: AtomicU32 = AtomicU32::new(0);

pub static IO_BUFFERS: RwLock<Vec<buffers::ConsoleBuffers>> = RwLock::new(Vec::new());

pub fn register_console_manager(id: TaskID) {
    CONSOLE_MANAGER_TASK.store(id.into(), Ordering::SeqCst);
}

pub fn get_console_manager_id() -> TaskID {
    TaskID::new(CONSOLE_MANAGER_TASK.load(Ordering::SeqCst))
}

static CONSOLE_SIGNAL: AtomicU32 = AtomicU32::new(0);

pub fn wake_console_manager() {
    CONSOLE_SIGNAL.fetch_add(1, Ordering::SeqCst);
    futex_wake(VirtualAddress::new(CONSOLE_SIGNAL.as_ptr() as u32), 1);
}

pub fn manager_task() -> ! {
    let text_buffer_base = map_memory(
        None,
        0x1000,
        MemoryBacking::Direct(PhysicalAddress::new(0xb8000)),
    )
    .unwrap();

    let mut conman = ConsoleManager::new(text_buffer_base);

    conman.clear_screen();
    conman.render_top_bar();

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

        // read pending bytes for each console, and process them
        conman.process_buffers();

        conman.update_cursor();

        conman.update_clock();

        futex_wait(
            VirtualAddress::new(CONSOLE_SIGNAL.as_ptr() as u32),
            0,
            Some(1000),
        );
        CONSOLE_SIGNAL.store(0, Ordering::SeqCst);
    }
}

pub fn init_console() {
    register_console_manager(create_kernel_task(manager_task, Some("CONMAN")));
}

pub fn console_ready() {
    crate::command::start_command(0);
}
