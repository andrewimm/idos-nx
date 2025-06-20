use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::sync::Arc;
use spin::RwLock;

use crate::{
    io::handle::Handle,
    memory::address::{PhysicalAddress, VirtualAddress},
    sync::wake_set::WakeSet,
    task::{
        actions::memory::map_memory, id::TaskID, memory::MemoryBacking, switching::get_current_task,
    },
};

static CONSOLE_MANAGER: RwLock<Option<(TaskID, Arc<WakeSet>)>> = RwLock::new(None);

pub fn register_console_manager(wake_set: Handle) -> Result<VirtualAddress, ()> {
    if CONSOLE_MANAGER.read().is_some() {
        return Err(());
    }
    {
        let current_task_lock = get_current_task();
        let current_task = current_task_lock.write();
        let task_id = current_task.id;
        let wake_set = current_task.wake_sets.get(wake_set).cloned().ok_or(())?;
        CONSOLE_MANAGER.write().replace((task_id, wake_set));
    }

    let buffer_phys =
        PhysicalAddress::new(unsafe { &label_input_buffer_start as *const () as u32 });
    let buffer_page =
        map_memory(None, 0x1000, MemoryBacking::Direct(buffer_phys)).map_err(|_| ())?;

    Ok(buffer_page)
}

pub fn wake_console_manager() {
    if let Some((_, wake_set)) = CONSOLE_MANAGER.read().as_ref() {
        wake_set.wake();
    }
}

#[allow(improper_ctypes)]
extern "C" {
    #[link_name = "__input_buffers"]
    static label_input_buffer_start: ();
}

#[repr(C)]
pub struct InputBuffer<const N: usize> {
    write_index: AtomicUsize,
    read_index: AtomicUsize,
    buffer: [u8; N],
}

impl<const N: usize> InputBuffer<N> {
    pub fn next_index(&self, current: usize) -> usize {
        (current + 1) % N
    }

    pub fn write(&self, value: u8) -> bool {
        let write_index = self.write_index.load(Ordering::Relaxed);
        let next_index = self.next_index(write_index);
        let read_index = self.read_index.load(Ordering::Acquire);
        if next_index == read_index {
            return false;
        }
        unsafe {
            let data_ptr: *mut u8 = &self.buffer[0] as *const u8 as u32 as *mut u8;
            let dest = data_ptr.offset(write_index as isize);
            core::ptr::write_volatile(dest, value);
        }
        self.write_index.store(next_index, Ordering::Release);
        true
    }

    pub fn read(&self) -> Option<u8> {
        let read_index = self.read_index.load(Ordering::Relaxed);
        let write_index = self.write_index.load(Ordering::Acquire);
        if read_index == write_index {
            return None;
        }
        let value = unsafe {
            let data_ptr: *const u8 = self.buffer.as_ptr();
            let src = data_ptr.offset(read_index as isize);
            core::ptr::read_volatile(src)
        };
        let next_index = self.next_index(read_index);
        self.read_index.store(next_index, Ordering::Release);
        return Some(value);
    }
}

const INPUT_BUFFERS_TOTAL_SIZE: usize = 0x1000;
pub const INPUT_BUFFER_SIZE: usize =
    (INPUT_BUFFERS_TOTAL_SIZE - core::mem::size_of::<AtomicUsize>() * 4) / 2;

unsafe fn get_buffers() -> (
    &'static mut InputBuffer<INPUT_BUFFER_SIZE>,
    &'static mut InputBuffer<INPUT_BUFFER_SIZE>,
) {
    let keyboard_start = &label_input_buffer_start as *const () as usize + 0xc000_0000;
    let keyboard_ptr = keyboard_start as *mut InputBuffer<INPUT_BUFFER_SIZE>;
    let mouse_start = keyboard_start + core::mem::size_of::<InputBuffer<INPUT_BUFFER_SIZE>>();
    let mouse_ptr = mouse_start as *mut InputBuffer<INPUT_BUFFER_SIZE>;

    (&mut *keyboard_ptr, &mut *mouse_ptr)
}

pub fn write_key_action(action_byte: u8, keycode_byte: u8) {
    let (keyboard_buffer, _) = unsafe { get_buffers() };
    keyboard_buffer.write(action_byte);
    keyboard_buffer.write(keycode_byte);
    wake_console_manager();
}

pub fn write_mouse_action(mouse_data: u8, dx: u8, dy: u8) {
    let (_, mouse_buffer) = unsafe { get_buffers() };
    mouse_buffer.write(mouse_data);
    mouse_buffer.write(dx);
    mouse_buffer.write(dy);
    wake_console_manager();
}
