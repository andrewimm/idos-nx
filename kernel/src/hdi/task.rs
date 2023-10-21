use crate::task::id::TaskID;
use crate::task::actions::lifecycle::create_task;
use super::HandleDrivenInterface;

pub struct TaskHDI {
    child: TaskID,
    exit_code: Option<u32>,
}

impl TaskHDI {
    pub fn child_exited(&mut self, child: TaskID, code: u32) {
        if self.child != child {
            return;
        }
        self.exit_code = Some(code);
    }
}

impl HandleDrivenInterface for TaskHDI {
    type HandleResult = TaskID;

    fn create_handle() -> Self::HandleResult {
        let child = create_task();
    }

    fn run_op(op: &super::HandleOp) {
        
    }
}
