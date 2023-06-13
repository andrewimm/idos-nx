//! Device driver for Intel e1000 ethernet controller, which is provded by
//! qemu and other emulators.

use crate::memory::address::PhysicalAddress;
use crate::task::actions::lifecycle::create_kernel_task;
use crate::task::actions::memory::map_memory;
use crate::task::actions::yield_coop;
use crate::task::memory::MemoryBacking;

use super::controller::E1000Controller;

fn run_driver() -> ! {
    let mmio_address = map_memory(
        None,
        0x10000,
        MemoryBacking::Direct(PhysicalAddress::new(0xfebc0000)),
    ).unwrap();

    crate::kprint!("Mapped PCI memory at {:?}\n", mmio_address);

    let mut controller = E1000Controller::new(mmio_address);

    let mac = controller.get_mac_address();
    crate::kprint!(
        "Ethernet MAC: {:X}:{:X}:{:X}:{:X}:{:X}:{:X}\n",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5],
    );

    loop {
        crate::task::actions::lifecycle::wait_for_io(None);
        yield_coop();
    }
}

pub fn install_driver() {
    // TODO: actually crawl the device tree and look for supported PCI devices
    // Then, use the BAR registers to find the appropriate IO port numbers, etc

    let task = create_kernel_task(run_driver);
}
