use crate::arch::rdtsc;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use super::id::{AtomicTaskID, TaskID};
use super::map::get_task;
use super::state::{RunState, Task};

/// All kernel code referring to the "current" task will use this TaskID
static CURRENT_ID: AtomicTaskID = AtomicTaskID::new(0);

static LAST_SWITCH: AtomicU32 = AtomicU32::new(0);
static LAST_SWITCH_DELTA: AtomicU32 = AtomicU32::new(0);

pub fn init(page_directory: PhysicalAddress) -> VirtualAddress {
    let mut idle_task = Task::create_initial_task();
    let idle_id = idle_task.id;
    idle_task.page_directory = page_directory;
    crate::kprint!("Initial pagedir {:?}\n", page_directory);
    super::map::insert_task(idle_task);

    let (_, tsc) = rdtsc();
    LAST_SWITCH.store(tsc, Ordering::SeqCst);

    super::scheduling::create_cpu_scheduler(0, idle_id, false)
}

pub fn get_current_id() -> TaskID {
    CURRENT_ID.load(core::sync::atomic::Ordering::SeqCst)
}

pub fn get_current_task() -> Arc<RwLock<Task>> {
    let current_id = get_current_id();
    let entry = super::map::get_task(current_id).expect("Current task does not exist");
    entry.clone()
}

pub fn update_timeouts(ms: u32) {
    super::map::for_each_task(|lock| {
        if let Some(mut task) = lock.try_write() {
            if task.update_timeout(ms) {
                // task resumed, put it back in the scheduler
                super::scheduling::reenqueue_task(task.id);
            }
        }
    });
}

pub fn clean_up_task(id: TaskID) {
    // Step 1: Close all open handles
    close_task_handles(id);

    // Step 2: Unmap all memory regions (frees physical frames)
    unmap_task_memory(id);

    // Step 3: Free page table frames and page directory frame
    free_task_page_tables(id);
}

/// Close all open handles for a terminated task. For bound file handles,
/// issues fire-and-forget close requests to drivers. We don't wait for
/// results — the async completion would just try to notify a dead task,
/// and `request_complete` handles missing tasks gracefully.
fn close_task_handles(id: TaskID) {
    use crate::io::async_io::IOType;
    use crate::io::filesystem::driver_close;

    let io_entries: Vec<(u32, Arc<IOType>)> = {
        let task_lock = match get_task(id) {
            Some(t) => t,
            None => return,
        };
        let task = task_lock.read();
        task.open_handles
            .iter()
            .filter_map(|(_handle, &io_index)| {
                task.async_io_table
                    .get(io_index)
                    .map(|entry| (io_index, entry.io_type.clone()))
            })
            .collect()
    };

    for (io_index, io_type) in &io_entries {
        if let IOType::File(ref file_io) = **io_type {
            let Some((driver_id, instance)) = file_io.get_binding() else {
                continue;
            };
            let op_id = file_io.next_op_id();
            if let Some(Err(e)) = driver_close(driver_id, instance, (id, *io_index, op_id)) {
                crate::kprintln!("Task {:?}: close error: {:?}", id, e);
            }
        }
    }
}

/// Unmap all memory regions for a terminated task, freeing physical frames.
fn unmap_task_memory(id: TaskID) {
    use super::actions::memory::unmap_memory_for_task;

    // Collect all region addresses and sizes, then release the lock before unmapping
    let regions: Vec<(VirtualAddress, u32)> = {
        let task_lock = match get_task(id) {
            Some(t) => t,
            None => return,
        };
        let task = task_lock.read();
        task.memory_mapping
            .drain_regions()
            .into_iter()
            .map(|r| {
                let size = (r.size + 0xfff) & 0xfffff000; // round up to page boundary
                (r.address, size)
            })
            .collect()
    };

    for (addr, size) in regions {
        if let Err(e) = unmap_memory_for_task(id, addr, size) {
            crate::kprintln!("Task {:?}: unmap error at {:?}: {:?}", id, addr, e);
        }
    }
}

/// Free the page table frames and page directory frame for a terminated task.
/// Must be called after all user-space pages have been unmapped.
fn free_task_page_tables(id: TaskID) {
    use crate::memory::physical::release_frame;
    use crate::memory::virt::page_table::PageTable;
    use crate::memory::virt::scratch::UnmappedPage;

    let page_directory_addr = {
        let task_lock = match get_task(id) {
            Some(t) => t,
            None => return,
        };
        let addr = task_lock.read().page_directory;
        addr
    };

    // Walk user-space page directory entries (0..768) and free any page table frames
    {
        let unmapped_dir = UnmappedPage::map(page_directory_addr);
        let page_dir = PageTable::at_address(unmapped_dir.virtual_address());
        for i in 0..768 {
            let entry = page_dir.get(i);
            if entry.is_present() {
                let table_frame_addr = entry.get_address();
                if let Err(e) = release_frame(table_frame_addr) {
                    crate::kprintln!(
                        "Task {:?}: failed to free page table frame {:?}: {:?}",
                        id,
                        table_frame_addr,
                        e
                    );
                }
            }
        }
    }

    // Free the page directory frame itself
    if let Err(e) = release_frame(page_directory_addr) {
        crate::kprintln!(
            "Task {:?}: failed to free page directory {:?}: {:?}",
            id,
            page_directory_addr,
            e
        );
    }
}

/// Execute a context switch to another task. If that task does not exist, the
/// method will panic.
/// In addition to updating relevant pointers to the new Task's ID, the actual
/// switch involves:
///   * Pushing all state onto the current task's kernel stack
///   * Executing `call` to push
///   * Saving the current stack pointer to
///   * Changing the stack pointer to the next Task's $esp
///   * Executing `ret` to pop from the next Task's stack
///   * Popping register state to resume execution in the next Task
/// When a Task is switched out, all of its state is stored in its own kernel
/// stack. When the kernel decides to switch back into that Task, its stack
/// pointer is resurrected and all the registers are popped, making it seem as
/// though the call to the inner switch method never happened.
pub fn switch_to(id: TaskID) {
    // Uncomment this to debug switching:
    //crate::kprintln!("    SWITCH TO {:?}", id);

    // use variations in switch timing for random entropy
    let (_, current_tsc) = rdtsc();
    let switch_delta = current_tsc.wrapping_sub(LAST_SWITCH.load(Ordering::SeqCst));
    let old_delta = LAST_SWITCH_DELTA.load(Ordering::SeqCst);
    let jitter = (switch_delta as i32 - old_delta as i32) as u32;
    crate::random::add_entropy(jitter);
    LAST_SWITCH.store(current_tsc, Ordering::SeqCst);
    LAST_SWITCH_DELTA.store(switch_delta, Ordering::SeqCst);

    let (current_sp_addr, current_fpu_ptr): (u32, u32) = {
        let current_lock = get_current_task();
        let current = current_lock.read();
        (
            &(current.stack_pointer) as *const usize as u32,
            current.fpu_state.data.as_ptr() as u32,
        )
    };
    let next_task_lock = get_task(id).expect("Switching to task that does not exist");
    let (next_sp, pagedir_addr, stack_top, next_fpu_ptr) = {
        let next = next_task_lock.read();
        (
            next.stack_pointer as u32,
            next.page_directory.as_u32(),
            next.get_stack_top(),
            next.fpu_state.data.as_ptr() as u32,
        )
    };
    let next_task_state = next_task_lock.read().state;

    crate::arch::gdt::set_tss_stack_pointer(stack_top as u32);

    let _ = CURRENT_ID.swap(id, core::sync::atomic::Ordering::SeqCst);

    // Save outgoing task's FPU state, restore incoming task's
    unsafe {
        asm!("fxsave [{}]", in(reg) current_fpu_ptr, options(nostack));
        asm!("fxrstor [{}]", in(reg) next_fpu_ptr, options(nostack));
    }

    if let RunState::Initialized = next_task_state {
        {
            next_task_lock.write().state = RunState::Running;
        }
        unsafe {
            asm!(
                "push eax",
                "push ecx",
                "push edx",
                "push ebx",
                "push ebp",
                "push esi",
                "push edi",

                "call switch_init_inner",

                "pop edi",
                "pop esi",
                "pop ebp",
                "pop ebx",
                "pop edx",
                "pop ecx",
                "pop eax",

                in("eax") pagedir_addr,
                in("ecx") current_sp_addr,
                in("edx") next_sp,
            );
        }
    } else {
        unsafe {
            asm!(
                "push eax",
                "push ecx",
                "push edx",
                "push ebx",
                "push ebp",
                "push esi",
                "push edi",

                "call switch_inner",

                "pop edi",
                "pop esi",
                "pop ebp",
                "pop ebx",
                "pop edx",
                "pop ecx",
                "pop eax",

                in("eax") pagedir_addr,
                in("ecx") current_sp_addr,
                in("edx") next_sp,
            );
        }
    }
}

global_asm!(
    r#"
.global switch_inner

switch_inner:
    mov cr3, eax
    mov [ecx], esp
    mov esp, edx
    ret
"#
);

global_asm!(
    r#"
.global switch_init_inner

switch_init_inner:
    mov cr3, eax
    mov [ecx], esp
    mov esp, edx

    pop eax
    pop ecx
    pop edx
    pop ebx
    pop ebp
    pop esi
    pop edi
    iretd
"#
);
