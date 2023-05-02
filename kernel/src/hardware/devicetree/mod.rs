pub mod bus;
pub mod storage;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

pub type DeviceID = u32;

pub struct DeviceTree {
    next_id: DeviceID,
    root_node: DeviceID,
    nodes: BTreeMap<DeviceID, DeviceNode>,
}

impl DeviceTree {
    pub fn new(root_node: DeviceNode) -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(1, root_node);

        Self {
            next_id: 2,
            root_node: 1,
            nodes,
        }
    }

    pub fn get_root(&self) -> DeviceID {
        self.root_node
    }

    pub fn get_node(&self, id: DeviceID) -> Option<&DeviceNode> {
        self.nodes.get(&id)
    }

    pub fn get_node_mut(&mut self, id: DeviceID) -> Option<&mut DeviceNode> {
        self.nodes.get_mut(&id)
    }

    pub fn insert_node(&mut self, parent: DeviceID, node: DeviceNode) -> DeviceID {
        let id = self.next_id;
        self.next_id += 1;

        let parent_node = self.get_node_mut(parent).expect("Tried to install device on invalid parent node");

        parent_node.children.push(id);
        self.nodes.insert(id, node);

        id
    }
}

pub struct DeviceNode {
    pub node_type: DeviceNodeType,
    pub children: Vec<DeviceID>,
}

impl DeviceNode {
    pub fn new(node_type: DeviceNodeType) -> Self {
        Self {
            node_type,
            children: Vec::new(),
        }
    }

    pub fn root_pci_bus() -> Self {
        Self::new(DeviceNodeType::Bus(bus::BusType::PCI))
    }
}

pub enum DeviceNodeType {
    Bus(bus::BusType),
    Storage(storage::StorageController),

    Unknown,
}
