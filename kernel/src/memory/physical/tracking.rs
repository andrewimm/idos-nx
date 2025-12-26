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
    pub fn new() -> Self {
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
