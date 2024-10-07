use super::super::address::VirtualAddress;

pub struct PageIter {
    current_page: VirtualAddress,
    bytes_remaining: usize,
}

impl PageIter {
    pub fn for_vaddr_range(start: VirtualAddress, length: usize) -> Self {
        let page_start = start.prev_page_barrier();
        let delta = (start - page_start) as usize;

        Self {
            current_page: page_start,
            bytes_remaining: length + delta,
        }
    }
}

impl Iterator for PageIter {
    type Item = VirtualAddress;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes_remaining > 0 {
            let page_start = self.current_page;
            self.current_page = self.current_page + 0x1000;
            self.bytes_remaining = self.bytes_remaining.saturating_sub(0x1000);
            Some(page_start)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PageIter;
    use crate::memory::address::VirtualAddress;

    #[test_case]
    fn page_iter() {
        let mut iter = PageIter::for_vaddr_range(VirtualAddress::new(0x40), 0x1000);
        assert_eq!(iter.next(), Some(VirtualAddress::new(0)));
        assert_eq!(iter.next(), Some(VirtualAddress::new(0x1000)));
        assert_eq!(iter.next(), None);
    }
}
