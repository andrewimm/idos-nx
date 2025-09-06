use alloc::boxed::Box;
use alloc::string::String;

use crate::cleanup::wake_cleanup_resident;
use crate::io::async_io::IOType;

use super::super::id::TaskID;
use super::yield_coop;

pub fn create_kernel_task(task_body: fn() -> !, name: Option<&str>) -> TaskID {
    let task_id = create_task();
    let task_state_lock = super::super::map::get_task(task_id).unwrap();
    {
        let mut task_state = task_state_lock.write();
        task_state.set_entry_point(task_body);
        task_state.state = super::super::state::RunState::Running;
        task_state.filename = String::from(name.unwrap_or("KERNEL"));
    }

    super::super::scheduling::reenqueue_task(task_id);

    task_id
}

pub fn create_task() -> TaskID {
    let cur_id = super::super::switching::get_current_id();
    let task_id = super::super::map::get_next_task_id();
    let task_stack = super::super::stack::allocate_stack();
    let mut task_state = super::super::state::Task::new(task_id, cur_id, task_stack);
    task_state.page_directory = super::super::paging::create_page_directory();
    super::super::map::insert_task(task_state);
    task_id
}

pub fn create_idle_task(stack: Box<[u8]>) -> TaskID {
    let task_id = super::super::map::get_next_task_id();
    let mut task_state = super::super::state::Task::new(task_id, TaskID::new(0), stack);
    task_state.page_directory = super::super::paging::create_page_directory();
    task_state.filename = String::from("IDLE");
    super::super::map::insert_task(task_state);

    task_id
}

pub fn add_args<I, A>(id: TaskID, args: I)
where
    I: IntoIterator<Item = A>,
    A: AsRef<[u8]>,
{
    let task_lock = super::super::map::get_task(id).unwrap();
    task_lock.write().push_args(args);
}

/// An iterator designed to read and emit args passed by the add_args syscall
pub struct InMemoryArgsIterator {
    buffer_start: *const u8,
    total_length: usize,

    current_index: usize,
    current_offset: usize,
}

impl InMemoryArgsIterator {
    pub fn new(buffer_start: *const u8, total_length: usize) -> Self {
        Self {
            buffer_start,
            total_length,
            current_index: 0,
            current_offset: 0,
        }
    }
}

impl Iterator for InMemoryArgsIterator {
    type Item = &'static [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_offset >= self.total_length {
            return None;
        }

        unsafe {
            let size_ptr = self.buffer_start.add(self.current_offset) as *const u16;
            let arg_size = (*size_ptr as usize).min(self.total_length - self.current_offset);
            let arg_bytes = core::slice::from_raw_parts(
                self.buffer_start.add(self.current_offset + 2),
                arg_size,
            );
            self.current_offset += 2 + arg_size;
            self.current_index += 1;
            Some(arg_bytes)
        }
    }
}

pub fn terminate_id(id: TaskID, exit_code: u32) {
    let parent_id = {
        let terminated_task = super::super::map::get_task(id);
        match terminated_task {
            Some(task_lock) => {
                let mut task = task_lock.write();
                task.terminate();
                task.parent_id
            }
            None => return,
        }
    };

    let parent_task = super::super::map::get_task(parent_id);
    if let Some(parent_lock) = parent_task {
        let io_provider = parent_lock.read().async_io_table.get_task_io(id).clone();
        if let Some((_io_index, provider)) = io_provider {
            if let IOType::ChildTask(ref io) = *provider {
                io.task_exited(exit_code);
            }
        }
    }

    // notify the cleanup task
    wake_cleanup_resident();
}

pub fn terminate_task(id: TaskID, exit_code: u32) {
    terminate_id(id, exit_code);
}

pub fn terminate(exit_code: u32) -> ! {
    let cur_id = super::switching::get_current_id();
    terminate_id(cur_id, exit_code);
    yield_coop();
    unreachable!("Task has terminated");
}

pub fn exception() {
    let cur_id = super::switching::get_current_id();
    crate::kprint!("EXCEPTION! {:?}\n", cur_id);
    // TODO: implement exception handling

    terminate_id(cur_id, 255);
    yield_coop();
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    #[test_case]
    fn test_adding_args() {
        let task_id = super::create_task();

        let args = alloc::vec!["arg_one", "arg_2", "arg_three"];
        let total_length = args.iter().map(|s| s.len() + 2).sum::<usize>();

        let mut buffer = Vec::with_capacity(total_length);

        for arg in args.iter() {
            let arg_bytes = arg.as_bytes();
            let len = arg_bytes.len();
            buffer.push((len & 0xFF) as u8);
            buffer.push(((len >> 8) & 0xFF) as u8);
            buffer.extend_from_slice(arg_bytes);
        }

        assert_eq!(buffer.len(), total_length);

        let arg_iter = super::InMemoryArgsIterator::new(buffer.as_ptr(), total_length);
        super::add_args(task_id, args.iter());

        let task_lock = crate::task::map::get_task(task_id).unwrap();
        let task = task_lock.read();

        let collected_args = task.args.get_raw().clone();
        let mut expected_args = Vec::new();
        for s in args.iter() {
            expected_args.extend_from_slice(s.as_bytes());
            expected_args.push(0); // Add null terminator after each string
        }
        assert_eq!(collected_args, expected_args);
    }
}
