pub mod driver;

use crate::task::id::TaskID;

use self::driver::DriverID;

use super::async_io::AsyncOp;

pub fn get_driver_id_by_name(name: &str) -> Result<DriverID, ()> {
    Ok(DriverID::new(1))
}

pub fn send_driver_io_request(task: TaskID, driver: DriverID, op: AsyncOp) {
    op.complete(1);
}
