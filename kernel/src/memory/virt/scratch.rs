use core::sync::atomic::{AtomicU32, Ordering};
use crate::memory::address::{PhysicalAddress, VirtualAddress};

use super::page_table::PageTable;

pub const SCRATCH_TOP: usize = 0xffc00000;
pub const SCRATCH_PAGE_COUNT: usize = 4;
pub const SCRATCH_BOTTOM: usize = SCRATCH_TOP - (SCRATCH_PAGE_COUNT * 0x1000);

/// Bitmap recording which scratch pages have been currently allocated
static SCRATCH_PAGES: AtomicU32 = AtomicU32::new(0);

/// We use a region of pages beneath the topmost page for editing memory that
/// isn't mapped to the current task. This is typically used for creating page
/// tables for other tasks, or editing their initial memory.
/// To use these pages, we allocate UnmappedPage structs which mark a scratch
/// page as occupied, and release it when dropped.
pub struct UnmappedPage {
    pub address: PhysicalAddress,
    scratch_index: usize,
}

impl UnmappedPage {
    pub fn map(address: PhysicalAddress) -> UnmappedPage {
        let mut mask: u32 = 1;
        for i in 0..SCRATCH_PAGE_COUNT {
            let prev = SCRATCH_PAGES.fetch_or(mask, Ordering::SeqCst);
            if prev & mask == 0 {
                // Found an unused scratch table.
                // Because the top pagedir entry is self-mapped:
                //   - The top 0x1000 of memory will contain the pagedir
                //   - The next 0x1000 will map the 4MiB ending at 0xffc00000
                // This second-from-the-top table contains entries for 1024
                // pages, the highest of which are the scratch area. A specific
                // scratch page `x` will be found at entry `1023 - x`.
                let top_table = PageTable::at_address(VirtualAddress::new(0xffffe000));
                let entry = 1023 - i;
                top_table.get_mut(entry).set_address(address);
                top_table.get_mut(entry).set_present();
                let virtual_addr = VirtualAddress::new((SCRATCH_TOP - ((i + 1) * 0x1000)) as u32);
                super::invalidate_page(virtual_addr);

                return UnmappedPage {
                    address,
                    scratch_index: i,
                }
            }
            mask <<= 1;
        }
        panic!("There are no free scratch pages");
    }

    pub fn virtual_address(&self) -> VirtualAddress {
        VirtualAddress::new((SCRATCH_TOP - ((self.scratch_index + 1) * 0x1000)) as u32)
    }
}

impl Drop for UnmappedPage {
    fn drop(&mut self) {
        let mask = !(1 << self.scratch_index);
        // Mark the page as unused again by turning off the bit
        SCRATCH_PAGES.fetch_and(mask, Ordering::SeqCst);
    }
}
