use crate::task::actions::lifecycle::exception;

use super::stack::StackFrame;

/// Triggered when dividing by zero, or when the result is too large to fit in
/// the destination register.
#[no_mangle]
pub extern "x86-interrupt" fn div(stack_frame: StackFrame) {
    // send a soft interrupt to the current task indicating an arithmetic exception
    crate::kprint!("Divide by zero\n");
    exception();
}

/// Debug trap used for a number of tracing modes like single-step
#[no_mangle]
pub extern "x86-interrupt" fn debug(stack_frame: StackFrame) {
    panic!("Debug trap");
}

#[no_mangle]
pub extern "x86-interrupt" fn nmi(stack_frame: StackFrame) {
    panic!("NMI");
}

/// Triggered by the INT 3 instruction. Used to stop execution and alert a
/// debugger, if one is attached.
#[no_mangle]
pub extern "x86-interrupt" fn breakpoint(stack_frame: StackFrame) {
    let current_lock = crate::task::switching::get_current_task();
    // look for task that might be tracing this one

    panic!("Break");
}

#[no_mangle]
pub extern "x86-interrupt" fn overflow(stack_frame: StackFrame) {
    panic!("Overflow");
}

#[no_mangle]
pub extern "x86-interrupt" fn bound_exceeded(stack_frame: StackFrame) {
    panic!("BOUND Range Exceeded");
}

#[no_mangle]
pub extern "x86-interrupt" fn invalid_opcode(stack_frame: StackFrame) {
    panic!("Invalid Opcode");
}

#[no_mangle]
pub extern "x86-interrupt" fn fpu_not_available(stack_frame: StackFrame) {
    panic!("FPU not available");
}

#[no_mangle]
pub extern "x86-interrupt" fn double_fault(stack_frame: StackFrame, error: u32) {
    loop {}
}

#[no_mangle]
pub extern "x86-interrupt" fn invalid_tss(stack_frame: StackFrame, error: u32) {
    loop {}
}

#[no_mangle]
pub extern "x86-interrupt" fn segment_not_present(stack_frame: StackFrame, error: u32) {
    loop {}
}

#[no_mangle]
pub extern "x86-interrupt" fn stack_segment_fault(stack_frame: StackFrame, error: u32) {
    loop {}
}

#[no_mangle]
pub extern "x86-interrupt" fn gpf(stack_frame: StackFrame, error: u32) {
    panic!("GPF");
}

#[no_mangle]
pub extern "x86-interrupt" fn page_fault(stack_frame: StackFrame, error: u32) {
    crate::kprint!("Page Fault: {:#010X}\n", error);
    loop {}
}

