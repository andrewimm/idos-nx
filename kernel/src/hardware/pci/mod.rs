use super::devicetree::{DeviceTree, DeviceNode};

pub mod config;
pub mod devices;

pub fn init() {
    let root_node = DeviceNode::root_pci_bus();
    let mut device_tree = DeviceTree::new(root_node);
    config::enumerate(&mut device_tree);
}
