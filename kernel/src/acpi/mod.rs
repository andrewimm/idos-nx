pub mod madt;
pub mod rsdp;
pub mod sdt;
pub mod table;

use self::madt::{MADTEntryType, MADT};
use self::sdt::SDTHeader;
use self::table::TableHeader;
use crate::memory::address::PhysicalAddress;
use crate::task::paging::get_current_physical_address;
use alloc::vec::Vec;

struct LocalAPIC {
    id: u8,
    processor: u8,
}

pub fn init() {
    // While traversing the ACPI tables, we will pull out hardware information
    // necessary for boot.

    // The MADT table contains information on APICs, which will determine what
    // cores are available.
    let mut found_apics: Vec<LocalAPIC> = Vec::new();

    // start by searching for the RSDP
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
    for table_header in root_sdt.iter() {
        crate::kprintln!(
            "TABLE ADDR {:#X}",
            table_header as *const TableHeader as u32
        );
        crate::kprintln!(
            "ACPI: Visit {}",
            core::str::from_utf8(&table_header.signature).unwrap()
        );

        if &table_header.signature == b"APIC" {
            // multiple apic description table
            // parse the table to determine how many processors, APICs, and
            // I/O APICs are connected to the system

            let madt = MADT::at_address(table_header as *const TableHeader as u32);
            for entry in madt.iter() {
                match entry.refine() {
                    MADTEntryType::LocalAPIC(e) => {
                        // if the LAPIC is enabled, count this as another core
                        if e.flags & 1 != 0 {
                            found_apics.push(LocalAPIC {
                                id: e.apic_id,
                                processor: e.processor_id,
                            });
                        }

                        crate::kprintln!("     MADT: Found Local APIC");
                        crate::kprintln!("{:?}", e);
                    }
                    MADTEntryType::IOAPIC(e) => {
                        crate::kprintln!("    MADT: Found I/O APIC");
                        crate::kprintln!("{:?}", e);
                    }
                    MADTEntryType::InterruptSourceOverride(e) => {
                        crate::kprintln!("     MADT: Found interrupt source override");
                        crate::kprintln!("{:?}", e);
                    }
                    MADTEntryType::IONMI => {
                        crate::kprintln!("     MADT: Found I/O NMI");
                    }
                    MADTEntryType::LocalNMI => {
                        crate::kprintln!("     MADT: Found Local NMI");
                    }
                    MADTEntryType::Unknown => {
                        crate::kprintln!("     MADT: Found unknown entry");
                    }
                }
            }
        } else if &table_header.signature == b"FACP" {
            // fixed acpi description table
        } else if &table_header.signature == b"HPET" {
            // high precision timer
            // TODO: not supported
        }
    }

    if found_apics.len() > 1 {
        crate::kprintln!("Found {} AP(s), booting them", found_apics.len() - 1);
        let copy_addr = crate::hardware::cpu::copy_trampoline();
        let trampoline_paddr = get_current_physical_address(copy_addr).unwrap();
        crate::kprintln!("Trampoline exists at {:?}", trampoline_paddr);

        let mut apic_phys: u32;
        unsafe {
            let msr: u32 = 0x1b;
            core::arch::asm!("rdmsr", in("ecx") msr, out("eax") apic_phys, out("edx") _);
        }
        apic_phys &= 0xfffff000;
        crate::kprintln!("APIC PADDR: {:#X}", apic_phys);

        let lapic = crate::hardware::lapic::LocalAPIC::new(PhysicalAddress::new(apic_phys));
        for apic in found_apics.iter().skip(1) {
            // boot each AP
            crate::kprintln!("Booting AP, LAPIC ID {}", apic.id);
            crate::kprintln!("Send INIT IPI");
            lapic.set_icr((apic.id as u32) << 24, 0x4500);

            let sipi_addr = trampoline_paddr.as_u32() >> 12;
            crate::kprintln!("Send SIPI IPI");
            lapic.set_icr((apic.id as u32) << 24, 0x4600 | sipi_addr);
        }
    }
}
