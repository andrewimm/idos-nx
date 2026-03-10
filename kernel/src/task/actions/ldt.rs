use alloc::boxed::Box;

use idos_api::compat::LdtDescriptorParams;

use crate::arch::ldt::{LocalDescriptorTable, LDT_MAX_ENTRIES};
use crate::task::switching::get_current_task;

/// Initialize the LDT for the current task if it doesn't already have one.
fn ensure_ldt(task: &mut crate::task::state::Task) -> &mut LocalDescriptorTable {
    if task.ldt.is_none() {
        task.ldt = Some(Box::new(LocalDescriptorTable::new()));
    }
    task.ldt.as_mut().unwrap()
}

/// Syscall 0x08: Allocate an LDT descriptor.
/// Returns the selector (index << 3 | TI=1 | RPL=3), or 0xffff_ffff if full.
pub fn ldt_allocate() -> u32 {
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    let ldt = ensure_ldt(&mut task);
    match ldt.allocate() {
        Some(index) => ((index as u32) << 3) | 0x04 | 0x03,
        None => 0xffff_ffff,
    }
}

/// Syscall 0x09: Modify an LDT descriptor.
/// ebx = selector, ecx = pointer to LdtDescriptorParams.
/// Sets base, limit, access, and flags in one call. Returns 0 on success.
pub fn ldt_modify(selector: u32, params_ptr: *const LdtDescriptorParams) -> u32 {
    let index = (selector >> 3) as usize;
    if index == 0 || index >= LDT_MAX_ENTRIES {
        return 0xffff_ffff;
    }
    let params = unsafe { &*params_ptr };
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    let ldt = ensure_ldt(&mut task);
    ldt.entries[index].set_base(params.base);
    ldt.entries[index].set_limit(params.limit);
    ldt.entries[index].access = params.access;
    ldt.entries[index].flags_and_limit_high =
        (ldt.entries[index].flags_and_limit_high & 0x0f) | (params.flags & 0xf0);
    0
}

/// Syscall 0x0a: Free an LDT descriptor.
/// ebx = selector. Returns 0 on success, 0xffff_ffff on failure.
pub fn ldt_free(selector: u32) -> u32 {
    let index = (selector >> 3) as usize;
    let task_lock = get_current_task();
    let mut task = task_lock.write();
    match &mut task.ldt {
        Some(ldt) => {
            if ldt.free(index) { 0 } else { 0xffff_ffff }
        }
        None => 0xffff_ffff,
    }
}
