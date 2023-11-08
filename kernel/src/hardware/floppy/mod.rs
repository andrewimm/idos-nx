use crate::task::actions::lifecycle::create_kernel_task;

pub mod controller;
pub mod dev;
pub mod driver;
pub mod geometry;

pub fn install() {
    create_kernel_task(driver::run_driver, Some("FDDEV"));
}
