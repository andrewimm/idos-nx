use core::sync::atomic::Ordering;

use alloc::boxed::Box;

use crate::{
    loader::load_executable,
    memory::address::VirtualAddress,
    sync::futex::futex_wait,
    task::{
        actions::{handle::create_task, send_message},
        id::AtomicTaskID,
        messaging::Message,
        paging::get_current_physical_address,
    },
};

static GFX_TASK: AtomicTaskID = AtomicTaskID::new(0xffff_ffff);

pub fn register_graphics_driver(path: &str) {
    let (_, gfx_task) = create_task();
    load_executable(gfx_task, "C:\\GFX.ELF");

    GFX_TASK.swap(gfx_task, Ordering::SeqCst);
}

#[derive(Copy, Clone, Default)]
pub struct VbeModeInfo {
    pub width: u16,
    pub height: u16,
    pub pitch: u16,
    pub bpp: u8,
    pub framebuffer: u32,
}

pub fn get_vbe_mode_info(mode_info: &mut VbeModeInfo, mode: u16) {
    let gfx_task = GFX_TASK.load(Ordering::SeqCst);
    let mut signal = Box::<u32>::new(0);
    let signal_addr = VirtualAddress::new(&*signal as *const u32 as u32);
    let mode_info_addr = VirtualAddress::new(mode_info as *mut VbeModeInfo as u32);
    send_message(
        gfx_task,
        Message {
            unique_id: 0,
            message_type: 0x11, // get VBE mode info
            args: [
                mode as u32,
                get_current_physical_address(signal_addr).unwrap().as_u32(),
                get_current_physical_address(mode_info_addr)
                    .unwrap()
                    .as_u32(),
                0,
                0,
                0,
            ],
        },
        0xffff_ffff,
    );
    futex_wait(signal_addr, 0, None);
}

pub fn set_vbe_mode(mode: u16) {
    let gfx_task = GFX_TASK.load(Ordering::SeqCst);
    let mut signal = Box::<u32>::new(0);
    let signal_addr = VirtualAddress::new(&*signal as *const u32 as u32);
    send_message(
        gfx_task,
        Message {
            unique_id: 0,
            message_type: 0x12,
            args: [
                mode as u32,
                get_current_physical_address(signal_addr).unwrap().as_u32(),
                0,
                0,
                0,
                0,
            ],
        },
        0xffff_ffff,
    );
    futex_wait(signal_addr, 0, None);
}
