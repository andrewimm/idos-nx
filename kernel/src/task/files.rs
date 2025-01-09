use crate::io::filesystem::driver::DriverID;
use alloc::string::String;

#[derive(Clone)]
pub struct CurrentDrive {
    pub name: String,

    pub driver_id: DriverID,
}

impl CurrentDrive {
    pub fn empty() -> Self {
        Self {
            name: String::new(),
            driver_id: DriverID::new(0),
        }
    }
}
