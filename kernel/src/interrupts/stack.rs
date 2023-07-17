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

impl core::fmt::Debug for StackFrame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let eip = self.eip;
        let cs = self.cs;
        let eflags = self.eflags;
        f.write_fmt(
            core::format_args!(
                "StackFrame {{\n  eip: {:#x}\n  cs: {:#x}\n  eflags: {:#x}\n}}\n",
                eip,
                cs,
                eflags,
            )
        )
    }
}

/// SavedState stashes the running state of a task when it is interrupted.
/// Restoring these would allow the CPU to return to its pre-interrupt state
/// without the task ever knowing.
#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct SavedState {
    edi: u32,
    esi: u32,
    ebp: u32,
    ebx: u32,
    edx: u32,
    ecx: u32,
    eax: u32,
}

