use crate::memory::address::VirtualAddress;

#[derive(Clone, Debug)]
pub enum Relocation {
    /// Only type of relocation supported in a DOS EXE, adds a 16-bit offset to
    /// a word at a specific address
    DosExe(VirtualAddress, u16),

    /// Elf Relocation, with base address and info field
    ElfRel(VirtualAddress, u32),

    ElfRelA(VirtualAddress, u32, u32),
}

impl Relocation {
    pub fn get_address(&self) -> VirtualAddress {
        match self {
            Self::DosExe(addr, _) => *addr,
            Self::ElfRel(addr, _) => *addr,
            Self::ElfRelA(addr, _, _) => *addr,
        }
    }

    pub fn apply(&self) {
        match self {
            Self::DosExe(addr, offset) => {
                let ptr = addr.as_ptr_mut::<u16>();
                unsafe {
                    let prev = core::ptr::read_volatile(ptr);
                    core::ptr::write_volatile(ptr, prev.wrapping_add(*offset));
                }
            },
            _ => panic!("Unimplemented relocation"),
        }
    }
}
