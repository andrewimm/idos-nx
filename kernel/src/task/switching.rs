use crate::memory::address::PhysicalAddress;
use alloc::sync::Arc;
use core::arch::{asm, global_asm};
use spin::RwLock;

use super::id::{AtomicTaskID, TaskID};
use super::map::get_task;
use super::state::{RunState, Task};

/// All kernel code referring to the "current" task will use this TaskID
static CURRENT_ID: AtomicTaskID = AtomicTaskID::new(0);

pub fn init(page_directory: PhysicalAddress) {
    let mut idle_task = Task::create_initial_task();
    idle_task.page_directory = page_directory;
    crate::kprint!("Initial pagedir {:?}\n", page_directory);
    super::map::insert_task(idle_task);
}

pub fn get_current_id() -> TaskID {
    CURRENT_ID.load(core::sync::atomic::Ordering::SeqCst)
}

pub fn get_current_task() -> Arc<RwLock<Task>> {
    let current_id = get_current_id();
    let entry = super::map::get_task(current_id).expect("Current task does not exist");
    entry.clone()
}

/// Force the scheduler to switch to another task.
pub fn switch() {
    let next = find_next_running_task();
    match next {
        Some(id) => switch_to(id),
        None => (),
    }
}

/// Find another task to switch to. If none is available (typically, if the
/// idle task is running and all others are blocked), it will return None.
/// Right now, the switching logic is simple: find the next largest TaskID
/// after the current task. If there is no eligible task larger than the
/// current ID, switch to the earliest Task ID.
pub fn find_next_running_task() -> Option<TaskID> {
    let current = get_current_id();
    let mut first_runnable = None;
    let map = super::map::GLOBAL_TASK_MAP.read();
    for (id, task) in map.iter() {
        if *id == current {
            continue;
        }
        let can_resume = task.read().can_resume();
        if can_resume {
            if *id > current {
                return Some(*id);
            }
            if first_runnable.is_none() {
                first_runnable.replace(*id);
            }
        }
    }
    first_runnable
}

pub fn update_timeouts(ms: u32) {
    super::map::for_each_task(|lock| {
        if let Some(mut task) = lock.try_write() {
            task.update_timeout(ms);
        }
    });
}

pub fn clean_up_task(_id: TaskID) {
    // iterate over open handles and close them

    // TODO: add cleanup actions here (free remaining memory, etc)

    // At this point, the Task state will be Dropped, and all heap objects held
    // within the struct itself will be freed
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

    let current_sp_addr: u32 = {
        let current_lock = get_current_task();
        let current = current_lock.read();
        &(current.stack_pointer) as *const usize as u32
    };
    let next_task_lock = get_task(id).expect("Switching to task that does not exist");
    let (next_sp, pagedir_addr, stack_top) = {
        let next = next_task_lock.read();
        (
            next.stack_pointer as u32,
            next.page_directory.as_u32(),
            next.get_stack_top(),
        )
    };
    let next_task_state = next_task_lock.read().state;

    crate::arch::gdt::set_tss_stack_pointer(stack_top as u32);

    let _ = CURRENT_ID.swap(id, core::sync::atomic::Ordering::SeqCst);

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
