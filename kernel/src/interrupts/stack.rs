/// Each interrupts and exception places these values on the stack, so that the
/// previously running code can be re-entered when the interrupt ends.
#[repr(C, packed)]
pub struct StackFrame {
    pub eip: u32,
    pub cs: u32,
    pub eflags: u32,
}

impl StackFrame {
}
