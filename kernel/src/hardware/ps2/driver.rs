use core::sync::atomic::{AtomicU32, Ordering};

use alloc::vec::Vec;

use crate::memory::address::VirtualAddress;
use crate::sync::futex::futex_wait;
use crate::task::actions::yield_coop;
use crate::task::id::TaskID;
use crate::task::switching::get_task;
use crate::{collections::RingBuffer, console::wake_console_manager};

use super::keyboard::{KeyboardState, OPEN_KEYBOARD_HANDLES};

static KEYBOARD_BUFFER_RAW: [u8; 32] = [0; 32];
pub static KEYBOARD_BUFFER: RingBuffer<u8> = RingBuffer::for_buffer(&KEYBOARD_BUFFER_RAW);

static MOUSE_BUFFER_RAW: [u8; 32] = [0; 32];
pub static MOUSE_BUFFER: RingBuffer<u8> = RingBuffer::for_buffer(&MOUSE_BUFFER_RAW);

pub static DATA_READY: AtomicU32 = AtomicU32::new(0);

pub fn ps2_driver_task() -> ! {
    let mut ids_to_wake: Vec<TaskID> = Vec::new();

    let mut keyboard_bytes: Vec<u8> = Vec::new();
    let mut mouse_bytes: Vec<u8> = Vec::new();

    let mut keyboard_state = KeyboardState::new();

    loop {
        let mut wake_manager = false;

        loop {
            let maybe_action = match KEYBOARD_BUFFER.read() {
                Some(data) => keyboard_state.handle_scan_byte(data),
                None => break,
            };

            if let Some(action) = maybe_action {
                let [a, b] = action.to_raw();
                keyboard_bytes.push(a);
                keyboard_bytes.push(b);

                crate::console::INPUT_BUFFER.write(action);
                wake_manager = true;
            }
        }
        loop {
            match MOUSE_BUFFER.read() {
                Some(data) => {
                    crate::kprint!("M{:X}", data);
                    mouse_bytes.push(data);
                }
                None => break,
            }
        }

        if keyboard_bytes.len() > 0 {
            loop {
                if let Some(mut handles) = OPEN_KEYBOARD_HANDLES.try_write() {
                    for handle in handles.iter_mut() {
                        if !handle.is_reading {
                            continue;
                        }
                        for byte in keyboard_bytes.iter() {
                            handle.unread.push(*byte);
                        }
                        ids_to_wake.push(handle.reader_id);
                    }
                    break;
                } else {
                    yield_coop()
                }
            }
            keyboard_bytes.clear();
        }

        if mouse_bytes.len() > 0 {
            mouse_bytes.clear();
        }

        for task in ids_to_wake.iter() {
            if let Some(lock) = get_task(*task) {
                //lock.write().io_complete();
            }
        }
        ids_to_wake.clear();

        if wake_manager {
            wake_console_manager();
        }

        futex_wait(VirtualAddress::new(DATA_READY.as_ptr() as u32), 0, None);
        DATA_READY.fetch_sub(1, Ordering::SeqCst);
    }
}
