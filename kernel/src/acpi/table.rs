use crate::memory::address::PhysicalAddress;

#[repr(C, packed)]
pub struct TableHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

pub trait ACPITable {
    fn matches_signature(&self, signature: &[u8; 4]) -> bool {
        self.header().signature == *signature
    }

    fn header(&self) -> &'static TableHeader {
        unsafe { &*{ self as *const Self as *const () as *const TableHeader } }
    }

    fn is_checksum_valid(&self) -> bool {
        let mut sum: u8 = 0;
        let table_start = self as *const Self as *const () as u32;
        let table_end = table_start + self.header().length;
        let mut ptr = table_start;
        while ptr < table_end {
            sum = sum.wrapping_add(unsafe { *(ptr as *const u8) });
            ptr += 1;
        }
        sum == 0
    }
}
