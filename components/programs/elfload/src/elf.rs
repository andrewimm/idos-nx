//! ELF header structures for 32-bit ELF files.
//! Duplicated from kernel/src/loader/elf/headers.rs — de-duplication is future
//! work.

#[derive(Default)]
#[repr(C, packed)]
pub struct ElfHeader {
    pub magic: [u8; 4],
    pub bit_class: u8,
    pub endianness: u8,
    pub id_version: u8,
    pub os_abi: u8,
    pub abi_version: u8,
    pub _padding: [u8; 7],
    pub object_file_type: u16,
    pub machine: u16,
    pub elf_version: u32,
    pub entry_point: u32,
    pub program_header_offset: u32,
    pub section_header_offset: u32,
    pub flags: u32,
    pub header_size: u16,
    pub program_header_size: u16,
    pub program_header_count: u16,
    pub section_header_size: u16,
    pub section_header_count: u16,
    pub section_name_index: u16,
}

#[derive(Default)]
#[repr(C, packed)]
pub struct ProgramHeader {
    pub segment_type: u32,
    pub offset: u32,
    pub virtual_address: u32,
    pub physical_address: u32,
    pub file_size: u32,
    pub memory_size: u32,
    pub flags: u32,
    pub alignment: u32,
}

pub const ELF_MAGIC: [u8; 4] = [0x7f, 0x45, 0x4c, 0x46];
pub const SEGMENT_TYPE_LOAD: u32 = 1;
pub const SEGMENT_FLAG_WRITE: u32 = 1 << 1;
