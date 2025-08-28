use core::sync::atomic::{AtomicU32, Ordering};

use alloc::vec::Vec;

use crate::memory::address::VirtualAddress;
use crate::sync::futex::futex_wait;
use crate::task::actions::yield_coop;
use crate::{collections::RingBuffer, conman::wake_console_manager};

use super::keyboard::{KeyboardState, OPEN_KEYBOARD_HANDLES};

static KEYBOARD_BUFFER_RAW: [u8; 32] = [0; 32];
pub static KEYBOARD_BUFFER: RingBuffer<u8> = RingBuffer::for_buffer(&KEYBOARD_BUFFER_RAW);

static MOUSE_BUFFER_RAW: [u8; 32] = [0; 32];
pub static MOUSE_BUFFER: RingBuffer<u8> = RingBuffer::for_buffer(&MOUSE_BUFFER_RAW);

pub static DATA_READY: AtomicU32 = AtomicU32::new(0);

pub fn ps2_driver_task() -> ! {
    let mut keyboard_bytes: Vec<u8> = Vec::new();
    let mut mouse_bytes: Vec<u8> = Vec::new();

    let mut keyboard_state = KeyboardState::new();

    let mut mouse_packet_seq = 0;
    let mut mouse_packet: [u8; 3] = [0; 3];

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

                crate::conman::write_key_action(a, b);
                wake_manager = true;
            }
        }
        loop {
            match MOUSE_BUFFER.read() {
                Some(data) => {
                    mouse_packet[mouse_packet_seq] = data;
                    mouse_packet_seq += 1;
                    if mouse_packet_seq >= 3 {
                        crate::conman::write_mouse_action(
                            mouse_packet[0],
                            mouse_packet[1],
                            mouse_packet[2],
                        );
                        mouse_packet_seq = 0;
                    }
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

        if wake_manager {
            wake_console_manager();
        }

        futex_wait(VirtualAddress::new(DATA_READY.as_ptr() as u32), 0, None);
        DATA_READY.fetch_sub(1, Ordering::SeqCst);
    }
}
