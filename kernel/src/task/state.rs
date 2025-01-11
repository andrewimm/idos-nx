use crate::io::async_io::{AsyncIOTable, AsyncOpID, IOType};
use crate::io::driver::comms::IOResult;
use crate::io::handle::{Handle, HandleTable};
use crate::io::notify::NotifyQueue;
use crate::io::provider::IOProvider;
use crate::loader::environment::ExecutionEnvironment;
use crate::memory::address::PhysicalAddress;
use crate::time::system::{get_system_time, Timestamp};
use alloc::boxed::Box;
use alloc::string::String;

use super::args::ExecArgs;
use super::id::TaskID;
use super::memory::TaskMemory;
use super::messaging::{Message, MessagePacket, MessageQueue};
use super::registers::EnvironmentRegisters;
use super::stack::free_stack;

pub struct Task {
    /// The unique identifier for this Task
    pub id: TaskID,
    /// The ID of the parent Task
    pub parent_id: TaskID,
    /// Represents the current execution state of the task
    pub state: RunState,
    /// Timestamp when the Task was created
    pub created_at: Timestamp,

    /// A Box pointing to the kernel stack for this task. This stack will be
    /// used when the task is executing kernel-mode code.
    /// The stack Box is wrapped in an Option so that we can replace it with
    /// None before the Task struct is dropped. If any code attempts to drop
    /// the stack Box, it will panic because it was not created by the global
    /// allocator.
    pub kernel_stack: Option<Box<[u8]>>,
    /// Stores the kernel stack pointer when the task is swapped out. When the
    /// task is resumed by the scheduler, this address will be placed in $esp.
    /// Registers will be popped off the stack to resume the execution state
    /// of the task.
    pub stack_pointer: usize,
    /// Physical address of the task's page directory
    pub page_directory: PhysicalAddress,
    /// Stores all of the memory mappings for the Task
    pub memory_mapping: TaskMemory,

    /// Store Messages that have been sent to this task
    pub message_queue: MessageQueue,

    /// Store references to all open handles
    pub open_handles: HandleTable<u32>,
    /// Stores the actual active async IO objects
    pub async_io_table: AsyncIOTable,
    /// The set of active notify queues, which can be used to wait on handles
    pub notify_queues: HandleTable<NotifyQueue>,
    /// The name of the executable file running in the thread
    pub filename: String,
    /// The arguments passed to the executable
    pub args: ExecArgs,
}

impl Task {
    pub fn new(id: TaskID, parent_id: TaskID, stack: Box<[u8]>) -> Self {
        let stack_pointer = (stack.as_ptr() as usize) + stack.len() - core::mem::size_of::<u32>();
        Self {
            id,
            parent_id,
            state: RunState::Uninitialized,
            created_at: get_system_time().to_timestamp(),
            kernel_stack: Some(stack),
            stack_pointer,
            page_directory: PhysicalAddress::new(0),
            memory_mapping: TaskMemory::new(),
            message_queue: MessageQueue::new(),
            open_handles: HandleTable::new(),
            async_io_table: AsyncIOTable::new(),
            notify_queues: HandleTable::new(),
            filename: String::new(),
            args: ExecArgs::new(),
        }
    }

    pub fn create_initial_task() -> Self {
        let id = TaskID::new(0);
        let stack = super::stack::create_initial_stack();
        let mut task = Self::new(id, id, stack);
        task.state = RunState::Running;
        task.filename = String::from("IDLE");
        task
    }

    pub fn get_kernel_stack(&self) -> &Box<[u8]> {
        match &self.kernel_stack {
            Some(stack) => stack,
            None => panic!("Task does not have a stack"),
        }
    }

    pub fn get_kernel_stack_mut(&mut self) -> &mut Box<[u8]> {
        match &mut self.kernel_stack {
            Some(stack) => stack,
            None => panic!("Task does not have a stack"),
        }
    }

    pub fn get_stack_top(&self) -> usize {
        let stack = self.get_kernel_stack();
        (stack.as_ptr() as usize) + stack.len()
    }

    pub fn reset_stack_pointer(&mut self) {
        self.stack_pointer = self.get_stack_top();
    }

    /// Push a u8 value onto the kernel stack
    pub fn stack_push_u8(&mut self, value: u8) {
        self.stack_pointer -= 1;
        let esp = self.stack_pointer;
        let stack = self.get_kernel_stack_mut();
        let stack_start = stack.as_ptr() as usize;
        let offset = esp - stack_start;
        stack[offset] = value;
    }

    pub fn stack_push_u32(&mut self, value: u32) {
        self.stack_pointer -= 4;
        let esp = self.stack_pointer;
        let stack = self.get_kernel_stack_mut();
        let stack_start = stack.as_ptr() as usize;
        let offset = esp - stack_start;
        stack[offset + 0] = ((value & 0x000000ff) >> 0) as u8;
        stack[offset + 1] = ((value & 0x0000ff00) >> 8) as u8;
        stack[offset + 2] = ((value & 0x00ff0000) >> 16) as u8;
        stack[offset + 3] = ((value & 0xff000000) >> 24) as u8;
    }

    pub fn initialize_registers(&mut self) {
        self.stack_push_u32(0);
        self.stack_push_u32(0);
        self.stack_push_u32(0);
        self.stack_push_u32(0);
        self.stack_push_u32(0);
        self.stack_push_u32(0);
        self.stack_push_u32(0);
    }

    pub fn set_entry_point(&mut self, f: fn() -> !) {
        self.initialize_registers();
        self.stack_push_u32(f as *const () as u32);
    }

    /// Determine if the scheduler can re-enter this task
    pub fn can_resume(&self) -> bool {
        match self.state {
            RunState::Initialized => true,
            RunState::Running => true,
            RunState::Resuming(_) => true,
            _ => false,
        }
    }

    pub fn make_runnable(&mut self) {
        if let RunState::Uninitialized = self.state {
            self.state = RunState::Initialized;
        }
    }

    /// End all execution of the task, and mark its resources as available for
    /// cleanup
    pub fn terminate(&mut self) {
        self.state = RunState::Terminated;
    }

    pub fn is_terminated(&self) -> bool {
        match self.state {
            RunState::Terminated => true,
            _ => false,
        }
    }

    pub fn update_timeout(&mut self, ms: u32) {
        match self.state {
            RunState::Blocked(Some(t), block_type) => {
                self.state = if t <= ms {
                    RunState::Running
                } else {
                    RunState::Blocked(Some(t - ms), block_type)
                };
            }
            _ => (),
        }
    }

    pub fn sleep(&mut self, timeout_ms: u32) {
        if let RunState::Running = self.state {
            self.state = RunState::Blocked(Some(timeout_ms), BlockType::Sleep);
        } else {
            panic!("Cannot sleep a non-running task");
        }
    }

    pub fn read_message(&mut self, current_ticks: u32) -> (Option<MessagePacket>, bool) {
        self.message_queue.read(current_ticks)
    }

    pub fn read_message_blocking(
        &mut self,
        current_ticks: u32,
        timeout: Option<u32>,
    ) -> (Option<MessagePacket>, bool) {
        let (first_read, has_more) = self.message_queue.read(current_ticks);
        if first_read.is_some() {
            return (first_read, has_more);
        }
        // Nothing in the queue, block until something arrives
        self.state = RunState::Blocked(timeout, BlockType::Message);
        (None, false)
    }

    /// Place a Message in this task's queue. If the task is currently blocked
    /// on reading the message queue, it will resume running.
    /// Each message is accompanied by an expiration time (in system ticks),
    /// after which point the message is considered invalid.
    pub fn receive_message(
        &mut self,
        current_ticks: u32,
        from: TaskID,
        message: Message,
        expiration_ticks: u32,
    ) {
        self.message_queue
            .add(from, message, current_ticks, expiration_ticks);
        self.handle_incoming_messages();

        match self.state {
            RunState::Blocked(_, BlockType::Message) => {
                self.state = RunState::Running;
            }
            _ => (),
        }
    }

    pub fn handle_incoming_messages(&mut self) {
        if let Some(io_index) = self
            .async_io_table
            .handle_incoming_messages(&mut self.message_queue)
        {
            self.io_action_notify(io_index);
        }
    }

    /// Wait for a child process with the specified ID to return
    pub fn wait_for_child(&mut self, id: TaskID, timeout: Option<u32>) {
        self.state = RunState::Blocked(timeout, BlockType::WaitForChild(id));
    }

    pub fn wait_for_io(&mut self, timeout: Option<u32>) {
        self.state = RunState::Blocked(timeout, BlockType::IO);
    }

    /// Notify the task that a child task has terminated with an exit code
    pub fn child_terminated(&mut self, id: TaskID, exit_code: u32) {
        match self.async_io_table.get_task_io(id) {
            Some((io_index, mutex)) => {
                let notify = if let IOType::ChildTask(ref io) = *mutex {
                    io.task_exited(exit_code);
                    true
                } else {
                    false
                };
                if notify {
                    self.io_action_notify(io_index);
                }
            }
            _ => (),
        }

        let waiting_on = match self.state {
            RunState::Blocked(_, BlockType::WaitForChild(wait_id)) => wait_id,
            _ => return,
        };
        if id == waiting_on {
            self.state = RunState::Resuming(exit_code);
        }
    }

    pub fn wait_on_notify_queue(&mut self, handle: Handle, timeout: Option<u32>) {
        match self.notify_queues.get_mut(handle) {
            Some(queue) => {
                if queue.is_ready() {
                    return;
                }
            }
            None => return,
        }
        self.state = RunState::Blocked(timeout, BlockType::Notify(handle));
    }

    pub fn io_action_notify(&mut self, io_index: u32) {
        let waiting_on = match self.state {
            RunState::Blocked(_, BlockType::Notify(handle)) => Some(handle),
            _ => None,
        };
        for (index, queue) in self.notify_queues.iter_mut() {
            if queue.contains(io_index) {
                if waiting_on == Some(index) {
                    self.state = RunState::Running;
                } else {
                    queue.mark_ready();
                }
            }
        }
        let waiting_on = match self.state {
            RunState::Blocked(_, BlockType::Notify(handle)) => handle,
            _ => return,
        };
        match self.notify_queues.get(waiting_on) {
            Some(queue) => {
                if queue.contains(io_index) {
                    self.state = RunState::Running;
                }
            }
            None => return,
        }
    }

    pub fn async_io_complete(&mut self, io_index: u32, op_id: AsyncOpID, return_value: IOResult) {
        crate::kprintln!("IO COMPLETE {} {:?} {:?}", io_index, op_id, return_value);
        let should_notify = match self.async_io_table.get(io_index) {
            Some(async_io) => match *async_io.io_type {
                IOType::File(ref fp) => {
                    fp.complete_op(io_index, op_id, return_value);
                    true
                }
                IOType::Interrupt(ref ip) => {
                    ip.interrupt_notify();
                    true
                }
                _ => false,
            },
            _ => false,
        };

        if should_notify {
            self.io_action_notify(io_index);
        }
    }

    pub fn io_complete(&mut self) {
        match self.state {
            RunState::Blocked(_, BlockType::IO) => {
                self.state = RunState::Running;
            }
            _ => return,
        }
    }

    pub fn resume_from_wait(&mut self) -> u32 {
        match self.state {
            RunState::Resuming(code) => {
                self.state = RunState::Running;
                return code;
            }
            _ => 0,
        }
    }

    pub fn unblock(&mut self) {
        match self.state {
            RunState::Blocked(_, _) => {
                self.state = RunState::Running;
            }
            _ => return,
        }
    }

    pub fn push_arg(&mut self, arg: &str) {
        self.args.add(arg);
    }

    pub fn push_args<I, A>(&mut self, args: I)
    where
        I: IntoIterator<Item = A>,
        A: AsRef<str>,
    {
        for arg in args {
            self.args.add(arg.as_ref());
        }
    }

    pub fn attach_executable(&mut self, env: ExecutionEnvironment) {
        let ExecutionEnvironment {
            registers,
            relocations,
            segments,
            require_vm,
        } = env;
        self.memory_mapping.set_execution_segments(segments);
        self.memory_mapping.set_relocations(relocations);

        let mut flags = 0;

        if require_vm {
            flags |= 0x20000;
        }

        let esp_start = registers.esp.unwrap_or(0xc0000000);
        let esp = esp_start - self.args.stack_size() as u32;

        let registers = EnvironmentRegisters {
            eax: registers.eax.unwrap_or(0),
            ecx: registers.ecx.unwrap_or(0),
            edx: registers.edx.unwrap_or(0),
            ebx: registers.ebx.unwrap_or(0),
            ebp: registers.ebp.unwrap_or(0),
            esi: registers.esi.unwrap_or(0),
            edi: registers.edi.unwrap_or(0),

            eip: registers.eip,
            cs: registers.cs.unwrap_or(0x18 | 3),
            flags,
            esp,
            ss: registers.ss.unwrap_or(0x20 | 3),

            ds: registers.ds.unwrap_or(0x20 | 3),
            es: registers.es.unwrap_or(0x20 | 3),
            fs: 0,
            gs: 0,
        };

        self.stack_push_u32(registers.gs);
        self.stack_push_u32(registers.fs);
        self.stack_push_u32(registers.es);
        self.stack_push_u32(registers.ds);
        self.stack_push_u32(registers.ss);
        self.stack_push_u32(registers.esp);
        self.stack_push_u32(registers.flags);
        self.stack_push_u32(registers.cs);
        self.stack_push_u32(registers.eip);
        self.stack_push_u32(registers.edi);
        self.stack_push_u32(registers.esi);
        self.stack_push_u32(registers.ebp);
        self.stack_push_u32(registers.ebx);
        self.stack_push_u32(registers.edx);
        self.stack_push_u32(registers.ecx);
        self.stack_push_u32(registers.eax);
    }

    pub fn has_executable(&self) -> bool {
        false
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        let stack = self.kernel_stack.take();
        if let Some(b) = stack {
            free_stack(b);
        }
    }
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
#[derive(Copy, Clone)]
pub enum RunState {
    /// The Task has been created, but is not ready to be executed
    Uninitialized,
    /// The Task is executable, but has not run yet. This requires some special
    /// code to safely switch into from another running task
    Initialized,
    /// The Task can be safely run by the scheduler
    Running,
    /// The Task has ended, but still needs to be cleaned up
    Terminated,
    /// The Task is blocked on some condition, with an optional timeout
    Blocked(Option<u32>, BlockType),
    /// The Task is resuming from a Blocked state with a return code
    Resuming(u32),
}

impl core::fmt::Display for RunState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Uninitialized => f.write_str("Uninit"),
            Self::Initialized => f.write_str("Init"),
            Self::Running | Self::Resuming(_) => f.write_str("Run"),
            Self::Terminated => f.write_str("Term"),
            Self::Blocked(_, BlockType::Sleep) => f.write_str("Sleep"),
            Self::Blocked(_, BlockType::Message) => f.write_str("WaitMsg"),
            Self::Blocked(_, BlockType::WaitForChild(_)) => f.write_str("WaitTask"),
            Self::Blocked(_, BlockType::IO) => f.write_str("WaitIO"),
            Self::Blocked(_, BlockType::Notify(_)) => f.write_str("NotifyQueue"),
        }
    }
}

/// A task may block on a variety of hardware or software conditions. The
/// BlockType describes why the task is blocked, and how it can be resumed.
#[derive(Copy, Clone)]
pub enum BlockType {
    /// The Task is sleeping for a fixed period of time, stored in the timeout
    Sleep,
    /// The Task is waiting for a Message from another task
    Message,
    /// The Task is waiting for a Child Task to return
    WaitForChild(TaskID),
    /// The Task is blocked on async IO
    IO,

    /// The task is blocked on a notify queue
    Notify(Handle),
}
