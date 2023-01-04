use super::id::TaskID;

pub struct Task {
    /// The unique identifier for this Task
    pub id: TaskID,
    /// Represents the current execution state of the task
    pub state: RunState,
}

/// RunState represents the current state of the task, and determines how the
/// task scheduler treats it. It is mostly used to represent the ways that an
/// existing task may not be actively running.
///
/// A task is initially created with an Uninitialized state. Until an
/// executable program is attached, or the task is explicitly marked as ready,
/// the kernel assumes there is no code to run, so the task is ignored.
///
/// When a task is Running, the kernel assumes that it can be safely executed.
/// The scheduler will consider this task as a candidate for the next one to
/// run.
///
/// When a program crashes, exits, or is killed by a soft interrupt, it moves
/// to a Terminated state. This allows the task data to remain in memory until
/// the kernel is able to notify its parent and clean up the resources
/// associated with the terminated task. A kernel-level task regularly walks
/// the task map and handles any terminated tasks.
///
/// A task becomes Blocked when it wants to pause execution and yield the CPU
/// to other tasks. This may be waiting for a fixed amount of time (sleeping)
/// or blocking until hardware or another task is ready. The Blocked state
/// contains information on what conditions will allow the task to resume
/// execution, as well as an optional timeout. This allows every blocking
/// operation to 
pub enum RunState {
    /// The Task has been created, but is not ready to be executed
    Uninitialized,
    /// The Task can be safely run by the scheduler
    Running,
    /// The Task has ended, but still needs to be cleaned up
    Terminated,
    /// The Task is blocked on some condition, with an optional timeout
    Blocked(Option<usize>, BlockType),
}

/// A task may block on a variety of hardware or software conditions. The
/// BlockType describes why the task is blocked, and how it can be resumed.
pub enum BlockType {
    /// The Task is sleeping for a fixed period of time, stored in the timeout
    Sleep,
}
