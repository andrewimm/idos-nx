use alloc::string::String;

use crate::cleanup::wake_cleanup_resident;
use crate::io::async_io::IOType;

use super::super::id::TaskID;
use super::yield_coop;

pub fn create_kernel_task(task_body: fn() -> !, name: Option<&str>) -> TaskID {
    let task_id = create_task();
    let task_state_lock = super::super::switching::get_task(task_id).unwrap();
    {
        let mut task_state = task_state_lock.write();
        task_state.set_entry_point(task_body);
        task_state.state = super::super::state::RunState::Running;
        task_state.filename = String::from(name.unwrap_or("KERNEL"));
    }

    task_id
}

pub fn create_task() -> TaskID {
    let cur_id = super::super::switching::get_current_id();
    let task_id = super::super::switching::get_next_id();
    let task_stack = super::super::stack::allocate_stack();
    let mut task_state = super::super::state::Task::new(task_id, cur_id, task_stack);
    task_state.page_directory = super::super::paging::create_page_directory();
    super::switching::insert_task(task_state);
    task_id
}

pub fn add_args<I, A>(id: TaskID, args: I)
where
    I: IntoIterator<Item = A>,
    A: AsRef<str>,
{
    let task_lock = super::super::switching::get_task(id).unwrap();
    task_lock.write().push_args(args);
}

pub fn terminate_id(id: TaskID, exit_code: u32) {
    let parent_id = {
        let terminated_task = super::switching::get_task(id);
        match terminated_task {
            Some(task_lock) => {
                let mut task = task_lock.write();
                task.terminate();
                task.parent_id
            }
            None => return,
        }
    };

    let parent_task = super::switching::get_task(parent_id);
    if let Some(parent_lock) = parent_task {
        // TODO: delete this once all uses of blocking on child are deleted
        parent_lock.write().child_terminated(id, exit_code);

        let io_provider = parent_lock.read().async_io_table.get_task_io(id).clone();
        if let Some((io_index, provider)) = io_provider {
            if let IOType::ChildTask(ref io) = *provider {
                io.task_exited(io_index, exit_code);
            }
        }
    }

    // notify the cleanup task
    wake_cleanup_resident();
}

pub fn terminate(exit_code: u32) -> ! {
    let cur_id = super::switching::get_current_id();
    terminate_id(cur_id, exit_code);
    yield_coop();
    unreachable!("Task has terminated");
}

pub fn wait_for_child(id: TaskID, timeout: Option<u32>) -> u32 {
    let current_lock = super::switching::get_current_task();
    current_lock.write().wait_for_child(id, timeout);
    yield_coop();
    let code = current_lock.write().resume_from_wait();
    code
}

pub fn wait_for_io(timeout: Option<u32>) {
    let current_lock = super::switching::get_current_task();
    current_lock.write().wait_for_io(timeout);
    yield_coop();
}

pub fn exception() {
    let cur_id = super::switching::get_current_id();
    crate::kprint!("EXCEPTION! {:?}\n", cur_id);
    // TODO: implement exception handling

    terminate_id(cur_id, 255);
    yield_coop();
}
