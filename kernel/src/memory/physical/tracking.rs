//! Allocated Frame Tracker
//! In order to properly ref-count pages that are allocated to tasks, we need a
//! data structure that can quickly determine if a frame has already been
//! allocated, and increment a count if so. We also need to be able to remove
//! references and understand if the frame is no longer in use.

use super::super::address::PhysicalAddress;
use alloc::boxed::Box;

/// Number of bits consumed per level of the radix tree
const RADIX_BITS: usize = 5;
/// Number of children per node (2^RADIX_BITS)
const FANOUT: usize = 1 << RADIX_BITS; // 32
/// Number of levels in the tree (20 bits / 5 bits per level)
const TREE_DEPTH: usize = 4;
/// Page size bits (assumes 4KB pages)
const PAGE_BITS: usize = 12;

/// A node in the radix tree. The value is only Some for a leaf node.
struct Node<T: Sized> {
    value: Option<T>,
    /// Children array, allocated lazily when first child is inserted
    children: Option<Box<[Option<Node<T>>; FANOUT]>>,
}

impl<T> Node<T> {
    fn new() -> Self {
        Node {
            value: None,
            children: None,
        }
    }

    fn with_value(value: T) -> Self {
        Node {
            value: Some(value),
            children: None,
        }
    }

    fn ensure_children(&mut self) {
        if self.children.is_none() {
            // Create array filled with None using const generic
            let arr: [Option<Node<T>>; FANOUT] = [const { None }; FANOUT];
            self.children = Some(Box::new(arr));
        }
    }
}

/// Inner type stored at each leaf. Defining it as a static type here makes it
/// easier to update in the future.
type AddressTreeInner = usize;

/// A radix tree for storing reference counts to physical frames.
/// The tree can upsert an address, add a reference if the entry already exists,
/// or decrement/remove the entry.
pub struct AddressTree {
    root: Option<Node<AddressTreeInner>>,
}

impl AddressTree {
    pub const fn new() -> Self {
        AddressTree { root: None }
    }

    /// Extract the 5-bit index for a given level (0-3) from an address
    #[inline]
    fn extract_index(addr: PhysicalAddress, level: usize) -> usize {
        assert!(level < TREE_DEPTH);
        let page_num = addr.as_u32() >> PAGE_BITS; // Remove page offset bits [11:0]
        let shift = (TREE_DEPTH - 1 - level) * RADIX_BITS;
        ((page_num >> shift) & ((1 << RADIX_BITS) - 1)) as usize
    }

    /// Add a reference to the provided address, returning the resulting
    /// reference count
    pub fn add_reference(&mut self, addr: PhysicalAddress) -> AddressTreeInner {
        assert!(addr.as_u32() & 0xfff == 0, "Address must be page-aligned");
        let mut current: &mut Node<AddressTreeInner> = match self.root {
            Some(ref mut node) => node,
            None => {
                self.root = Some(Node::new());
                self.root.as_mut().unwrap()
            }
        };

        // Navigate through levels 0-2 (interior nodes)
        for level in 0..(TREE_DEPTH - 1) {
            let index = Self::extract_index(addr, level);
            current.ensure_children();

            let children = current.children.as_mut().unwrap();
            if children[index].is_none() {
                children[index] = Some(Node::new());
            }

            current = children[index].as_mut().unwrap();
        }

        // At level 3 (leaf), insert the value
        let index = Self::extract_index(addr, TREE_DEPTH - 1);
        current.ensure_children();

        let children = current.children.as_mut().unwrap();
        if children[index].is_none() {
            children[index] = Some(Node::with_value(1));
            return 1;
        }

        let leaf = children[index].as_mut().unwrap();
        let new_value = leaf.value.expect("Leaf node must have a value") + 1;
        leaf.value.replace(new_value);
        new_value
    }

    /// Increment the ref count, but only if the address already exists.
    /// Returns the new ref count if it existed, or None if not.
    pub fn add_reference_if_exists(&mut self, addr: PhysicalAddress) -> Option<AddressTreeInner> {
        assert!(addr.as_u32() & 0xfff == 0, "Address must be page-aligned");
        let mut current = self.root.as_mut()?;

        // Navigate through levels 0-2 (interior nodes)
        for level in 0..(TREE_DEPTH - 1) {
            let index = Self::extract_index(addr, level);
            let children = current.children.as_mut()?;
            current = children[index].as_mut()?;
        }

        // At level 3 (leaf), increment the value if it exists
        let index = Self::extract_index(addr, TREE_DEPTH - 1);
        let children = current.children.as_mut()?;
        let leaf = children[index].as_mut()?;
        let new_value = leaf.value.expect("Leaf node must have a value") + 1;
        leaf.value.replace(new_value);
        Some(new_value)
    }

    /// Check if an address is present in the tree
    pub fn contains(&self, addr: PhysicalAddress) -> bool {
        let mut current = match self.root.as_ref() {
            Some(node) => node,
            None => return false,
        };

        // Navigate through all levels
        for level in 0..TREE_DEPTH {
            let index = Self::extract_index(addr, level);
            let children = match current.children.as_ref() {
                Some(c) => c,
                None => return false,
            };
            current = match children[index].as_ref() {
                Some(child) => child,
                None => return false,
            };
        }

        true
    }

    pub fn get_reference_count(&self, addr: PhysicalAddress) -> Option<AddressTreeInner> {
        let mut current = match self.root.as_ref() {
            Some(node) => node,
            None => return None,
        };

        // Navigate through all levels
        for level in 0..TREE_DEPTH {
            let index = Self::extract_index(addr, level);
            let children = match current.children.as_ref() {
                Some(c) => c,
                None => return None,
            };
            current = match children[index].as_ref() {
                Some(child) => child,
                None => return None,
            };
        }

        current.value
    }

    /// Remove a reference from the given address, returning the new reference
    /// count if present. If the reference count reaches zero, the entry is
    /// removed from the tree by setting the leaf to None.
    /// If the address was not present, returns None.
    pub fn remove_reference(&mut self, addr: PhysicalAddress) -> Option<AddressTreeInner> {
        let mut current = self.root.as_mut()?;

        // Navigate to the second-to-last level
        for level in 0..(TREE_DEPTH - 1) {
            let index = Self::extract_index(addr, level);
            let children = current.children.as_mut()?;
            current = children[index].as_mut()?;
        }

        // current.children may contain the leaf node
        let leaf_index = Self::extract_index(addr, TREE_DEPTH - 1);
        let children = current.children.as_mut()?;
        let leaf_node = children[leaf_index].as_mut()?;

        let current_value = leaf_node.value.expect("Leaf node must have a value");
        if current_value > 1 {
            let new_value = current_value - 1;
            leaf_node.value.replace(new_value);
            Some(new_value)
        } else {
            // Remove the leaf node entirely
            children[leaf_index] = None;
            Some(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn new_tree_is_empty() {
        let tree = AddressTree::new();
        let addr = PhysicalAddress::new(0x1000);
        assert!(!tree.contains(addr));
    }

    #[test_case]
    fn add_reference_creates_entry() {
        let mut tree = AddressTree::new();
        let addr = PhysicalAddress::new(0x1000);

        let count = tree.add_reference(addr);
        assert_eq!(count, 1);
        assert!(tree.contains(addr));
    }

    #[test_case]
    fn add_reference_increments_existing() {
        let mut tree = AddressTree::new();
        let addr = PhysicalAddress::new(0x2000);

        let count = tree.add_reference(addr);
        assert_eq!(count, 1);

        let count = tree.add_reference(addr);
        assert_eq!(count, 2);

        let count = tree.add_reference(addr);
        assert_eq!(count, 3);

        assert!(tree.contains(addr));
    }

    #[test_case]
    fn if_exists_returns_none_for_new_address() {
        let mut tree = AddressTree::new();
        let addr = PhysicalAddress::new(0x3000);

        let result = tree.add_reference_if_exists(addr);
        assert_eq!(result, None);
        assert!(!tree.contains(addr));
    }

    #[test_case]
    fn if_exists_increments_existing() {
        let mut tree = AddressTree::new();
        let addr = PhysicalAddress::new(0x4000);

        tree.add_reference(addr);

        let count = tree.add_reference_if_exists(addr);
        assert_eq!(count, Some(2));
        assert!(tree.contains(addr));
    }

    #[test_case]
    fn remove_reference_decrements_count() {
        let mut tree = AddressTree::new();
        let addr = PhysicalAddress::new(0x5000);

        tree.add_reference(addr);
        tree.add_reference(addr);
        let count = tree.add_reference(addr);
        assert_eq!(count, 3);

        let count = tree.remove_reference(addr);
        assert_eq!(count, Some(2));
        assert!(tree.contains(addr));
    }

    #[test_case]
    fn remove_reference_removes_entry_at_zero() {
        let mut tree = AddressTree::new();
        let addr = PhysicalAddress::new(0x6000);

        tree.add_reference(addr);

        let count = tree.remove_reference(addr);
        assert_eq!(count, Some(0));
        assert!(!tree.contains(addr));
    }

    #[test_case]
    fn remove_reference_from_nonexistent_returns_none() {
        let mut tree = AddressTree::new();
        let addr = PhysicalAddress::new(0x7000);

        let result = tree.remove_reference(addr);
        assert_eq!(result, None);
    }

    #[test_case]
    fn multiple_addresses_independent() {
        let mut tree = AddressTree::new();
        let addr1 = PhysicalAddress::new(0x8000);
        let addr2 = PhysicalAddress::new(0x9000);
        let addr3 = PhysicalAddress::new(0xa000);

        tree.add_reference(addr1);
        tree.add_reference(addr2);
        tree.add_reference(addr2);
        tree.add_reference(addr3);
        tree.add_reference(addr3);
        tree.add_reference(addr3);

        assert!(tree.contains(addr1));
        assert!(tree.contains(addr2));
        assert!(tree.contains(addr3));

        assert_eq!(tree.remove_reference(addr2), Some(1));
        assert!(tree.contains(addr1));
        assert!(tree.contains(addr2));
        assert!(tree.contains(addr3));

        assert_eq!(tree.remove_reference(addr1), Some(0));
        assert!(!tree.contains(addr1));
        assert!(tree.contains(addr2));
        assert!(tree.contains(addr3));
    }

    #[test_case]
    fn addresses_with_different_radix_indices() {
        let mut tree = AddressTree::new();
        // Create addresses that differ at different radix tree levels
        let addr1 = PhysicalAddress::new(0x00001000); // Level 3 index 1
        let addr2 = PhysicalAddress::new(0x00020000); // Level 2 index 1
        let addr3 = PhysicalAddress::new(0x00400000); // Level 1 index 1
        let addr4 = PhysicalAddress::new(0x08000000); // Level 0 index 1

        tree.add_reference(addr1);
        tree.add_reference(addr2);
        tree.add_reference(addr3);
        tree.add_reference(addr4);

        assert!(tree.contains(addr1));
        assert!(tree.contains(addr2));
        assert!(tree.contains(addr3));
        assert!(tree.contains(addr4));
    }

    #[test_case]
    fn reference_cycle_add_remove_add() {
        let mut tree = AddressTree::new();
        let addr = PhysicalAddress::new(0xb000);

        tree.add_reference(addr);
        assert!(tree.contains(addr));

        tree.remove_reference(addr);
        assert!(!tree.contains(addr));

        tree.add_reference(addr);
        assert!(tree.contains(addr));
        assert_eq!(tree.add_reference(addr), 2);
    }

    #[test_case]
    fn test_high_reference_count() {
        let mut tree = AddressTree::new();
        let addr = PhysicalAddress::new(0xc000);

        for i in 1..=100 {
            let count = tree.add_reference(addr);
            assert_eq!(count, i);
        }

        assert!(tree.contains(addr));

        for i in (1..=99).rev() {
            let count = tree.remove_reference(addr);
            assert_eq!(count, Some(i));
        }

        assert!(tree.contains(addr));
        assert_eq!(tree.remove_reference(addr), Some(0));
        assert!(!tree.contains(addr));
    }

    #[test_case]
    fn test_contains_after_removal_of_different_address() {
        let mut tree = AddressTree::new();
        let addr1 = PhysicalAddress::new(0xd000);
        let addr2 = PhysicalAddress::new(0xe000);

        tree.add_reference(addr1);
        tree.add_reference(addr2);

        tree.remove_reference(addr1);

        assert!(!tree.contains(addr1));
        assert!(tree.contains(addr2));
    }

    #[test_case]
    fn add_reference_if_exists_after_removal() {
        let mut tree = AddressTree::new();
        let addr = PhysicalAddress::new(0xf000);

        tree.add_reference(addr);
        tree.remove_reference(addr);

        let result = tree.add_reference_if_exists(addr);
        assert_eq!(result, None);
    }

    #[test_case]
    fn test_adjacent_pages() {
        let mut tree = AddressTree::new();
        let addr1 = PhysicalAddress::new(0x10000);
        let addr2 = PhysicalAddress::new(0x11000);
        let addr3 = PhysicalAddress::new(0x12000);

        tree.add_reference(addr1);
        tree.add_reference(addr2);
        tree.add_reference(addr3);

        assert!(tree.contains(addr1));
        assert!(tree.contains(addr2));
        assert!(tree.contains(addr3));

        tree.remove_reference(addr2);

        assert!(tree.contains(addr1));
        assert!(!tree.contains(addr2));
        assert!(tree.contains(addr3));
    }

    #[test_case]
    fn test_empty_tree_operations() {
        let mut tree = AddressTree::new();
        let addr = PhysicalAddress::new(0x13000);

        assert_eq!(tree.remove_reference(addr), None);
        assert_eq!(tree.add_reference_if_exists(addr), None);
        assert!(!tree.contains(addr));
    }
}
