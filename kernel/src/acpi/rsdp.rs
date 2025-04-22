use crate::memory::address::{AddressRange, PhysicalAddress, VirtualAddress};

/// RSDP - Root System Description Pointer
/// RSDP is the entry point for ACPI tables. Its location is unknown, but can
/// be found by searching for the "RSD PTR " signature string.
/// Once found, the revision field needs to be checked to see if it has the
/// extra fields, and then
#[repr(C, packed)]
#[allow(unused)]
pub struct RSDP {
    /// Identifying signature: "RSD PTR "
    signature: [u8; 8],
    /// checksum that, when added to all other bytes,
    checksum: u8,
    /// OEM-provided string
    oem_id: [u8; 6],
    /// revision number
    revision: u8,
    /// location of RSDT
    rsdt_address: PhysicalAddress,
    /// table size
    length: u32,
    /// address of XSDT when available. It's a 64-bit field, but we only use
    /// the bottom 32-bits as an address
    xsdt_address: PhysicalAddress,
    xsdt_address_cont: u32,
    /// computes checksum of v2 table
    checksum_extended: u8,
    reserved: [u8; 3],
}

impl RSDP {
    pub fn search<R: AddressRange<PhysicalAddress>>(search_range: R) -> Option<&'static RSDP> {
        let start = search_range.get_first();
        let end = search_range.get_last();
        let mut offset = 0;
        while start + offset < end {
            let search_addr = VirtualAddress::new(start.as_u32() + offset);
            unsafe {
                if core::slice::from_raw_parts(search_addr.as_ptr::<u8>(), 8) == b"RSD PTR " {
                    return Some(&*search_addr.as_ptr::<RSDP>());
                }
            }
            offset += 16;
        }
        None
    }

    pub fn get_system_table(&self) -> PhysicalAddress {
        if self.revision == 0 {
            self.rsdt_address
        } else {
            self.xsdt_address
        }
    }
}
