use alloc::{collections::BTreeMap, sync::Arc};
use core::arch::{asm, global_asm};
use core::ops::Deref;
use spin::RwLock;
use super::id::{TaskID, IdGenerator};
use super::state::Task;

/// A TaskMap makes it easy to look up a Task by its ID number
pub type TaskMap = BTreeMap<TaskID, Arc<RwLock<Task>>>;

pub static TASK_MAP: RwLock<TaskMap> = RwLock::new(BTreeMap::new());

/// This IdGenerator is used to create a unique ID for the next Task
pub static NEXT_ID: IdGenerator = IdGenerator::new();

/// All kernel code referring to the "current" task will use this TaskID
pub static CURRENT_ID: RwLock<TaskID> = RwLock::new(TaskID::new(0));

pub fn init() {
    let idle_task = Task::create_initial_task();
    let id: TaskID = idle_task.id;
    let entry = Arc::new(RwLock::new(idle_task));
    {
        let mut map = TASK_MAP.write();
        map.insert(id, entry);
    }
}

pub fn get_current_id() -> TaskID {
    *CURRENT_ID.read()
}

pub fn get_current_task() -> Arc<RwLock<Task>> {
    let current_id = get_current_id();
    let map = TASK_MAP.read();
    let entry = map.get(&current_id).expect("Current task does not exist");
    entry.clone()
}

pub fn get_task(id: TaskID) -> Option<Arc<RwLock<Task>>> {
    let map = TASK_MAP.read();
    map
        .get(&id)
        .as_deref()
        .map(|inner| inner.clone())
}

pub fn get_next_id() -> TaskID {
    NEXT_ID.next()
}

/// Cooperatively yield, forcing the scheduler to find another runnable task
pub fn yield_coop() {
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
    let map = TASK_MAP.read();
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

pub fn insert_task(task: Task) {
    let id = task.id;
    let entry = Arc::new(RwLock::new(task));
    {
        let mut map = TASK_MAP.write();
        map.insert(id, entry);
    }
}

pub fn update_timeouts(ms: u32) {
    let map = TASK_MAP.read();
    for (_, lock) in map.iter() {
        if let Some(mut task) = lock.try_write() {
            task.update_timeout(ms);
        }
    }
}

pub fn for_each_task<F>(f: F)
    where F: Fn(Arc<RwLock<Task>>) -> () {
    for (_, task) in TASK_MAP.read().iter() {
        f(task.clone());
    }
}

pub fn for_each_task_mut<F>(mut f: F)
    where F: FnMut(Arc<RwLock<Task>>) -> () {
    for (_, task) in TASK_MAP.read().iter() {
        f(task.clone());
    }
}

pub fn clean_up_task(id: TaskID) {
    let task_lock = {
        let mut task_map = TASK_MAP.write();
        match task_map.remove(&id) {
            Some(t) => t,
            None => return,
        }
    };

    let mut task = task_lock.write();
    crate::kprint!("Clean up {:?}\n", id);
    // TODO: add cleanup actions here (free remaining memory, etc)
    // Files should be release at termination time, not here

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
    crate::kprint!("SWITCH TO {:?}\r\n", id);
    let current_sp_addr: u32 = {
        let current_lock = get_current_task();
        let current = current_lock.read();
        &(current.stack_pointer) as *const usize as u32
    };
    let next_sp: usize = {
        let next_lock = get_task(id).expect("Switching to task that does not exist");
        let next = next_lock.read();
        next.stack_pointer
    };

    {
        *CURRENT_ID.write() = id;
    }

    unsafe {
        asm!(
            //"2:",
            //"jmp 2b",
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

            in("ecx") current_sp_addr,
            in("edx") next_sp,
        );
    }
}

global_asm!(r#"
.global switch_inner

switch_inner:
    mov [ecx], esp
    mov esp, edx
    ret
"#);
