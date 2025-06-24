use core::{
    arch::global_asm,
    sync::atomic::{AtomicU32, AtomicUsize, Ordering},
};

use crate::{
    memory::{
        address::{PhysicalAddress, VirtualAddress},
        virt::page_table::get_current_pagedir,
    },
    task::{
        actions::memory::{map_memory, unmap_memory_for_task},
        memory::MemoryBacking,
        switching::get_current_id,
    },
};

/// atomic count of CPU cores, incremented each time an AP boots
pub static CPU_COUNT: AtomicUsize = AtomicUsize::new(1);
/// counter for CPU ID, incremented and assigned to each new core
pub static CPU_ID: AtomicU32 = AtomicU32::new(1);

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

pub fn set_trampoline_data(
    mapped_to: VirtualAddress,
    pagedir: PhysicalAddress,
    stack_top: VirtualAddress,
    entrypoint: VirtualAddress,
) {
    extern "C" {
        fn trampoline_start();
        fn trampoline_data();
    }

    let trampoline_data_offset =
        trampoline_data as *const u32 as u32 - trampoline_start as *const u32 as u32;
    let trampoline_data_ptr = (mapped_to + trampoline_data_offset).as_ptr_mut::<TrampolineData>();
    let trampoline_data = unsafe { &mut *trampoline_data_ptr };

    let id = CPU_ID.fetch_add(1, Ordering::SeqCst);

    trampoline_data.pagedir = pagedir;
    trampoline_data.stack_top = stack_top;
    trampoline_data.id = id;
    trampoline_data.entrypoint = entrypoint;
}

#[repr(C, packed)]
struct TrampolineData {
    pagedir: PhysicalAddress,
    stack_top: VirtualAddress,
    id: u32,
    entrypoint: VirtualAddress,
}

pub fn cleanup_trampoline(mapping: VirtualAddress) {
    unmap_memory_for_task(get_current_id(), mapping, 0x1000).unwrap();
}

global_asm!(
    r#"
.code16
.global trampoline_start
.global trampoline_end
.global trampoline_data

trampoline_start:
    cli

    xor eax, eax
    mov ax, cs
    mov ds, ax

    // set up pagedir
    .equ data_offset, trampoline_data - trampoline_start
    mov ebx, [data_offset]
    and ebx, 0xfffff000
    mov cr3, ebx

    // compute relocations
    .equ gdtr_offset, gdtr - trampoline_start

    mov dx, cs
    shl dx, 4

    // the same trampoline is shared between all cores, so only the first one
    // needs to apply the relocations. There are many signs the relocations
    // have been computed. For example, if the GDT address is greater than
    // 0xFFF, it must have had some number of pages added to it.
    mov ax, [gdtr_offset + 2]
    cmp ax, 0xfff
    jg .relocations_done

    xor ebx, ebx
    lea bx, [gdtr_offset + 2]
    add word ptr [bx], dx

    .equ addr_offset, .jump_to_32 - trampoline_start + 1
    lea bx, [addr_offset]
    add word ptr [bx], dx

.relocations_done:
    lea bx, [gdtr_offset]
    lgdt [ebx]

    mov ebx, 0x00000001
    mov cr0, ebx

    xor bx, bx
    mov ds, bx

.jump_to_32:
    // Even if LLVM inline assembly accepted a far jump (which it doesn't),
    // we'd still have to compute the relocation manually. So that's what we'll
    // do.
    .byte 0xea // far jump opcode
    .word trampoline_32 - trampoline_start // destination address
    .word 0x08 // cs segment

.code32
trampoline_32:
    mov bx, 0x10
    mov ds, bx
    mov es, bx
    mov ss, bx

    lea ebx, [data_offset]
    add ebx, edx

    add ebx, 4
    mov esp, [ebx]

    mov edx, cr0
    or edx, 0x80000000
    mov cr0, edx

    push long ptr [ebx]
    add ebx, 4
    push long ptr [ebx]

    add ebx, 4
    call [ebx]

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
    .long 0x00000000 // initial stack
    .long 0x00000000 // processor ID
    .long 0x00000000 // kernel entrypoint

trampoline_end:
    "#
);
