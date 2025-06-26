//! Store Tasks and look them up

use alloc::{collections::BTreeMap, sync::Arc};
use spin::RwLock;

use super::{
    id::{IdGenerator, TaskID},
    state::Task,
};

/// A TaskMap makes it easy to look up a Task by its ID number
pub type TaskMap = BTreeMap<TaskID, Arc<RwLock<Task>>>;

/// Store all Tasks in the system
/// TODO: make this private once we modify find_next_running_task
pub static GLOBAL_TASK_MAP: RwLock<TaskMap> = RwLock::new(BTreeMap::new());

/// This IdGenerator is used to create a unique ID for the next Task
static NEXT_ID: IdGenerator = IdGenerator::new();

/// Fetch a task by its ID
pub fn get_task(id: TaskID) -> Option<Arc<RwLock<Task>>> {
    let map = GLOBAL_TASK_MAP.read();
    map.get(&id).cloned()
}

/// Remove the Task struct from the global map, returning it so that any
/// resources can be cleaned up.
pub fn take_task(id: TaskID) -> Option<Arc<RwLock<Task>>> {
    let mut map = GLOBAL_TASK_MAP.write();
    map.remove(&id)
}

/// Generate a TaskID for a new Task
pub fn get_next_task_id() -> TaskID {
    NEXT_ID.next()
}

/// Insert a new Task into the global map
pub fn insert_task(task: Task) {
    let id = task.id;
    let entry = Arc::new(RwLock::new(task));
    let mut map = GLOBAL_TASK_MAP.write();
    map.insert(id, entry);
}

/// Run a method on each task in the system
pub fn for_each_task<F>(f: F)
where
    F: Fn(&Arc<RwLock<Task>>),
{
    let map = GLOBAL_TASK_MAP.read();
    for task in map.values() {
        f(task);
    }
}

pub fn for_each_task_mutfn<F>(mut f: F)
where
    F: FnMut(&Arc<RwLock<Task>>),
{
    let map = GLOBAL_TASK_MAP.read();
    for task in map.values() {
        f(task);
    }
}

pub fn for_each_task_id<F>(mut f: F)
where
    F: FnMut(TaskID),
{
    let map = GLOBAL_TASK_MAP.read();
    for id in map.keys() {
        f(*id);
    }
}
