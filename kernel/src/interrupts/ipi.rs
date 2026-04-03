use crate::task::scheduling::{get_cpu_scheduler, get_lapic};

use super::stack::StackFrame;

/// The BSP is connected to the PIT. When it receives a timer interrupt, it
/// sends a secondary IPI to all of the other processors in the system. This
/// allows them to respond to the timer the same way the BSP does.
pub extern "x86-interrupt" fn pit_cascade(stack_frame: StackFrame) {
    let scheduler = get_cpu_scheduler();
    let is_user = stack_frame.cs & 3 != 0 || stack_frame.eflags & 0x20000 != 0;
    scheduler.record_tick(is_user);
    scheduler.tick();
    get_lapic().eoi();
}
