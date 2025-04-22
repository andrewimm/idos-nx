use crate::{acpi::sdt::SDTHeader, memory::address::PhysicalAddress};

pub mod rsdp;
pub mod sdt;

pub fn init() {
    let found_rsdp = match self::rsdp::RSDP::search(
        PhysicalAddress::new(0xe0000)..=PhysicalAddress::new(0xfffff),
    ) {
        Some(rsdp) => rsdp,
        None => {
            crate::kprintln!("ACPI: No RSDP found...");
            return;
        }
    };

    crate::kprintln!(
        "ACPI: RSDP found: {:#X}",
        found_rsdp as *const self::rsdp::RSDP as u32
    );

    let root_sdt = SDTHeader::at_address(found_rsdp.get_system_table());
    for sdt in root_sdt.sdt_iter() {
        crate::kprintln!("TABLE ADDR {:#X}", sdt as *const SDTHeader as u32);
        crate::kprintln!(
            "ACPI: Visit {}",
            core::str::from_utf8(&sdt.signature).unwrap()
        );
    }
}
