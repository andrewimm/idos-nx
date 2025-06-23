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

    xor eax, eax
    mov ax, cs
    mov ds, ax

    // set up pagedir
    .equ data_offset, trampoline_data - trampoline_start
    mov ebx, [data_offset]
    mov cr3, ebx

    // compute relocations
    mov ax, cs
    shl ax, 4

    .equ gdtr_offset, gdtr - trampoline_start
    xor ebx, ebx
    lea bx, [gdtr_offset + 2]
    add word ptr [bx], ax
    sub bx, 2
    lgdt [ebx]

    .equ addr_offset, .jump_to_32 - trampoline_start + 1
    lea bx, [addr_offset]
    add word ptr [bx], ax

    mov ebx, cr0
    or ebx, 0x00000001
    mov cr0, ebx

    xor ax, ax
    mov ds, ax

.jump_to_32:
    // Even if LLVM inline assembly accepted a far jump (which it doesn't),
    // we'd still have to compute the relocation manually. So that's what we'll
    // do.
    .byte 0xea // far jump opcode
    .word trampoline_32 - trampoline_start // destination address
    .word 0x08 // cs segment

.code32
trampoline_32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
.halt_loop:
    hlt
    jmp .halt_loop

gdtr:
    .word gdt_end - gdt
    .long gdt - trampoline_start

gdt:
    .quad 0

    .word 0xffff
    .word 0
    .byte 0
    .byte 0x9a
    .byte 0xcf
    .byte 0

    .word 0xffff
    .word 0
    .byte 0
    .byte 0x92
    .byte 0xcf
    .byte 0
gdt_end:

trampoline_data:
    .long 0x00000000 // pagedir address
    .long 0x10305070 // initial stack

trampoline_end:
    "#
);
