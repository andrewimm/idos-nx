//! Scheduling of tasks has been enhanced to support multi-core CPUs.
//! Each CPU has its own run queue and handles scheduling of eligible tasks.
//! In addition, all cores can pick up async tasklets, which are also added to
//! their queues.

use core::sync::atomic::Ordering;

use alloc::collections::VecDeque;
use spin::Mutex;

use super::{
    id::{AtomicTaskID, TaskID},
    map::get_task,
    switching::switch_to,
};

/// This struct is instantiated once per CPU core, and manages data necessary to
/// run and switch tasks on that core.
pub struct CPUScheduler {
    current_task: AtomicTaskID,
    idle_task: TaskID,
    pub work_queue: Mutex<VecDeque<WorkItem>>,
}

impl CPUScheduler {
    pub const fn new(idle_task: TaskID) -> Self {
        Self {
            current_task: AtomicTaskID::new(0),
            idle_task,
            work_queue: Mutex::new(VecDeque::new()),
        }
    }

    pub fn get_current_task(&self) -> TaskID {
        self.current_task.load(Ordering::SeqCst)
    }

    pub fn get_next_work_item(&self) -> Option<WorkItem> {
        self.work_queue.lock().pop_front()
    }

    pub fn reenqueue_work_item(&self, item: WorkItem) {
        self.work_queue.lock().push_back(item);
    }
}

pub enum WorkItem {
    Task(TaskID),
    Tasklet(Tasklet),
}

pub struct Tasklet {}

/// Eventually this needs to be placed in a statically available place per core
static CPU_DATA: CPUScheduler = CPUScheduler::new(TaskID::new(0));

/// Get the CPUScheduler instance for the current CPU
pub fn get_cpu_scheduler() -> &'static CPUScheduler {
    &CPU_DATA
}

/// Put a task back on any work queue, making it eligible for execution again.
pub fn reenqueue_task(id: TaskID) {
    crate::kprintln!("SCHEDULER: re-enqueue task {:?}", id);
    let scheduler = get_cpu_scheduler();
    scheduler.reenqueue_work_item(WorkItem::Task(id));
}

/// If the current task is still running, put it on the back of the CPU's work
/// queue. Then, pop the first item off of the work queue. If there is no
/// runnable task, switch to the current CPU's idle task.
pub fn switch() {
    let scheduler = get_cpu_scheduler();
    let current_id = scheduler.current_task.load(Ordering::SeqCst);

    if current_id != scheduler.idle_task {
        // if the current task isn't the idle task, and it is still running,
        // re-enqueue it
        let current_task = get_task(current_id);
        if let Some(task) = current_task {
            if task.read().can_resume() {
                scheduler.reenqueue_work_item(WorkItem::Task(current_id));
            }
        }
    }

    let switch_to_id = loop {
        // Pop the first item off the queue, if one exists.
        // It's not guaranteed that enqueued tasks are runnable, since their
        // state may have changed. If the popped task is not runnable, discard
        // the ID and fetch another one.
        match scheduler.get_next_work_item() {
            Some(WorkItem::Task(id)) => {
                if let Some(task_lock) = get_task(id) {
                    if task_lock.read().can_resume() {
                        break id;
                    }
                }
            }
            Some(WorkItem::Tasklet(_)) => panic!("Tasklet isn't supported"),
            None => {
                // if there's nothing in the queue, switch to the idle task
                break scheduler.idle_task;
            }
        }
    };

    if current_id == switch_to_id {
        return;
    }

    scheduler.current_task.swap(switch_to_id, Ordering::SeqCst);

    switch_to(switch_to_id);
}
