use core::{
    arch::global_asm,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    memory::address::VirtualAddress,
    task::{
        actions::memory::{map_memory, unmap_memory_for_task},
        memory::MemoryBacking,
        switching::get_current_id,
    },
};

/// atomic count of CPU cores, incremented each time a BSP or AP boots
pub static CPU_COUNT: AtomicUsize = AtomicUsize::new(0);
/// counter for CPU ID, incremented and assigned to each new core
pub static CPU_ID: AtomicUsize = AtomicUsize::new(0);

/// The trampoline code is bookended by `trampoline_start` and `trampoline_end`
/// symbols. We allocate physical memory, copy the code there, and fill in a
/// few critical variables. When an AP starts, it jumps to the physical address
/// of the trampoline code.
pub fn copy_trampoline() -> VirtualAddress {
    extern "C" {
        fn trampoline_start();
        fn trampoline_end();
    }

    let start = trampoline_start as *const () as usize;
    let end = trampoline_end as *const () as usize;
    let size = end - start;
    let mapped_to = map_memory(None, 0x1000, MemoryBacking::Anonymous).unwrap();
    let dest_slice = unsafe { core::slice::from_raw_parts_mut(mapped_to.as_ptr_mut::<u8>(), size) };
    let src_slice = unsafe { core::slice::from_raw_parts(trampoline_start as *const u8, size) };

    dest_slice.copy_from_slice(src_slice);

    mapped_to
}

pub fn cleanup_trampoline(mapping: VirtualAddress) {
    unmap_memory_for_task(get_current_id(), mapping, 0x1000).unwrap();
}

global_asm!(
    r#"
.code16
.global trampoline_start
.global trampoline_end
trampoline_start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    xor sp, sp

.halt_loop:
    hlt
    jmp .halt_loop

trampoline_end:
    "#
);
