use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::physical::allocate_frame;
use crate::memory::virt::page_table::PageTable;
use crate::memory::virt::scratch::UnmappedPage;

pub fn create_page_directory() -> PhysicalAddress {
    let addr = allocate_frame().unwrap().to_physical_address();
    // map the pagedir to a scratch page, and copy contents from the kernel
    // space of the current pagedir
    {
        let unmapped = UnmappedPage::map(addr);
        let current_dir = PageTable::at_address(VirtualAddress::new(0xfffff000));
        let new_dir = PageTable::at_address(unmapped.virtual_address());
        for i in 0..0x400 {
            *(new_dir.get_mut(i)) = *(current_dir.get(i));
        }
    }
    addr
}
