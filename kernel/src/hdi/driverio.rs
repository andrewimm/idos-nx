use super::HandleDrivenInterface;

pub struct DriverIO {
}

impl HandleDrivenInterface for DriverIO {
    fn create_handle() -> T {
        return ();
    }

    fn run_op(op: &super::HandleOp) {
        
    }
}
