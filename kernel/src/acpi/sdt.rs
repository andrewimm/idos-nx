use crate::memory::address::PhysicalAddress;

/// Structure for the header found on all ACPI System Data Tables
#[repr(C, packed)]
pub struct SDTHeader {
    /// The signature of the table, used as an identifying string
    pub signature: [u8; 4],
    /// The length of the table in bytes
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

impl SDTHeader {
    pub fn at_address(address: PhysicalAddress) -> &'static Self {
        unsafe { &*(address.as_u32() as *const SDTHeader) }
    }

    pub fn matches_signature(&self, signature: &[u8; 4]) -> bool {
        self.signature == *signature
    }

    pub fn is_checksum_valid(&self) -> bool {
        let mut sum: u8 = 0;
        let table_start = self as *const SDTHeader as u32;
        let table_end = table_start + self.length;
        let mut ptr = table_start;
        while ptr < table_end {
            sum = sum.wrapping_add(unsafe { *(ptr as *const u8) });
            ptr += 1;
        }
        sum == 0
    }

    pub fn get_sdt_list_start(&self) -> *const u32 {
        let first_pointer =
            self as *const SDTHeader as u32 + core::mem::size_of::<SDTHeader>() as u32;
        first_pointer as *const u32
    }

    pub fn sdt_iter(&self) -> SDTIter {
        SDTIter::new(
            self.get_sdt_list_start(),
            (self.length as usize - core::mem::size_of::<SDTHeader>())
                / core::mem::size_of::<u32>(),
        )
    }
}

/// Iterator for ACPI System Data Tables
pub struct SDTIter {
    root_address: *const u32,
    table_count: usize,
    current_index: usize,
}

impl SDTIter {
    pub fn new(root_address: *const u32, table_count: usize) -> Self {
        SDTIter {
            root_address,
            table_count,
            current_index: 0,
        }
    }
}

impl Iterator for SDTIter {
    type Item = &'static SDTHeader;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index >= self.table_count {
            return None;
        }
        unsafe {
            let next_table_location = self.root_address.offset(self.current_index as isize);
            let next_address: u32 = core::ptr::read_volatile(next_table_location);
            self.current_index += 1;

            Some(&*(next_address as *const SDTHeader))
        }
    }
}
