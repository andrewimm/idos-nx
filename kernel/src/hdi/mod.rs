//! Handle Driven Interface (HDI) is a method for async interaction with any
//! data source. It includes a very simple message-based abstraction for
//! sending a request to a driver and later confirming the result.
//! This is what allows file IO, net sockets, interrupts, and similar
//! read/write interfaces to share common logic, and will allow the OS to add
//! similar handle-driven interfaces without adding syscall bloat. Ultimately
//! the goal has been to keep the kernel surface small.

pub mod driverio;
pub mod task;

use alloc::boxed::Box;

use crate::task::paging::get_current_physical_address;

pub trait HandleDrivenInterface {
    type HandleResult;

    fn create_handle() -> Self::HandleResult;

    fn run_op(op: &HandleOp);
}

#[derive(Copy, Clone)]
pub struct HandleOp {
    pub op_code: u32,
    pub semaphore: PhysicalAddress,
    pub arg0: u32,
    pub arg1: u32,
    pub arg2: u32,
}

impl HandleOp {
    pub fn new(code: u32, semaphore_addr: u32, arg0: u32, arg1: u32, arg2: u32) -> Self {
        let phys_addr = get_current_physical_address(VirtualAddress::new(semaphore_addr)).unwrap();
        Self {
            op_code: code,
            semaphore: phys_addr,
            arg0,
            arg1,
            arg2,
        }
    }

    pub fn complete(&self, code: u32) {
        let phys_frame_start = self.semaphore.as_u32() & 0xfffff000;
        let semaphore_offset = self.semaphore.as_u32() - phys_frame_start;
        let unmapped_for_dir = UnmappedPage::map(PhysicalAddress::new(phys_frame_start));
        unsafe {
            let ptr = (unmapped_for_dir.virtual_address() + semaphore_offset).as_ptr::<AtomicU32>();
            (&*ptr).store(code, Ordering::SeqCst);
        }
    }
}

pub fn register_handle_source(source: Box<dyn HandleDrivenInterface>) -> u32 {
    0
}

pub fn run_op(handle_source: u32, op: &HandleOp) {
}


// =========

pub trait HandleProvider {
    fn add_op(&mut self, id: u32, op: HandleOp);
    fn get_op(&self, id: u32) -> Option<&HandleOp>;
    fn complete_op(&mut self, id: u32);
}

pub struct TaskHandle {

}

impl TaskHandle {
    pub fn for_task(id: TaskID) -> Self {
    }

    pub fn get_id(&self) -> TaskID {
    }

    pub fn task_exited(&mut self, code: u32) {
    }
}

impl HandleProvider for TaskHandle {
}

// ---

pub struct MessageQueueHandle {
}

impl MessageQueueHandle {
    pub fn new() -> Self {
    }

    pub fn message_ready(&mut self) -> bool {
    }
}
