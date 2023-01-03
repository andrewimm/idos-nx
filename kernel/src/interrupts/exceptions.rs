use super::stack::StackFrame;

#[no_mangle]
pub extern "x86-interrupt" fn gpf(stack_frame: StackFrame, error: u32) {
    loop {}
}
