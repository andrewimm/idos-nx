pub mod config;
pub mod devices;

pub fn init() {
    config::enumerate();
}
