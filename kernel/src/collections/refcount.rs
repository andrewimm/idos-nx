use alloc::collections::BTreeMap;

///
pub struct RefCountMap<T: core::cmp::Ord> {
    map: BTreeMap<T, u32>,
}

impl<T: core::cmp::Ord> RefCountMap<T> {
    pub const fn new() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    pub fn add_reference(&mut self, t: T) -> u32 {
        if let Some(count) = self.map.get_mut(&t) {
            *count += 1;
            return *count;
        }
        self.map.insert(t, 1);
        return 1;
    }

    pub fn remove_reference(&mut self, t: T) -> u32 {
        match self.map.get_mut(&t) {
            Some(count) => {
                *count = count.saturating_sub(1);
                if *count > 0 {
                    return *count;
                }
            },
            None => return 0,
        }
        // if we didn't early return, it means remove the entry completely
        self.map.remove(&t);
        return 0;
    }

    pub fn contains(&self, t: T) -> bool {
        self.map.contains_key(&t)
    }
}

#[cfg(test)]
mod tests {
    use crate::memory::address::PhysicalAddress;
    use super::RefCountMap;

    #[test_case]
    fn refcount_map() {
        let mut map = RefCountMap::new();

        assert!(!map.contains(PhysicalAddress::new(0xf0004000)));

        assert_eq!(map.add_reference(PhysicalAddress::new(0xf0004000)), 1);
        assert_eq!(map.add_reference(PhysicalAddress::new(0xf0004000)), 2);
        assert_eq!(map.add_reference(PhysicalAddress::new(0xc008)), 1);

        assert_eq!(map.remove_reference(PhysicalAddress::new(0xf0004000)), 1);
        assert_eq!(map.remove_reference(PhysicalAddress::new(0xc008)), 0);
        assert!(!map.contains(PhysicalAddress::new(0xc008)));
        assert!(map.contains(PhysicalAddress::new(0xf0004000)));
    }
}

