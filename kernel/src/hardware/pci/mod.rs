pub mod config;
pub mod devices;

use alloc::vec::Vec;
use spin::Once;

use self::devices::PciDevice;

static PCI_BUS: Once<Vec<PciDevice>> = Once::new();

pub fn get_bus_devices() -> &'static Vec<PciDevice> {
    PCI_BUS.call_once(|| {
        let mut devices = Vec::new();
        config::enumerate(&mut devices);
        devices
    })
}

