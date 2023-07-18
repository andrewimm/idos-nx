/// Each interrupts and exception places these values on the stack, so that the
/// previously running code can be re-entered when the interrupt ends.
#[repr(C, packed)]
pub struct StackFrame {
    pub eip: u32,
    pub cs: u32,
    pub eflags: u32,
}

impl StackFrame {
    fn as_ptr(&self) -> *mut u32 {
        self as *const StackFrame as u32 as *mut u32
    }

    pub fn set_eip(&self, eip: u32) {
        unsafe {
            core::ptr::write_volatile(self.as_ptr(), eip);
        }
    }

    pub fn add_eip(&self, delta: i32) {
        let value = (self.eip as i32 + delta) as u32;
        self.set_eip(value);
    }

    pub fn set_cs(&self, cs: u32) {
        unsafe {
            core::ptr::write_volatile(self.as_ptr().offset(1), cs);
        }
    }

    pub fn set_eflags(&self, eflags: u32) {
        unsafe {
            core::ptr::write_volatile(self.as_ptr().offset(2), eflags);
        }
    }
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

