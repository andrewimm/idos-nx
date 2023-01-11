#[repr(C, packed)]
pub struct ElfHeader {
    pub magic_number: [u8; 4],
    pub bit_class: u8,
    pub endianness: u8,
    pub identifier_version: u8,
    pub os_abi: u8,
    pub os_abi_version: u8,
    pub reserved_padding: [u8; 7],
    pub object_type: u16,
    pub machine: u16,
    pub elf_version: u32,
    pub entry_point: u32,
    pub program_header_location: u32,
    pub section_header_location: u32,
    pub flags: u32,
    pub header_size: u16,
    pub program_header_entry_size: u16,
    pub program_header_entry_count: u16,
    pub section_header_entry_size: u16,
    pub section_header_entry_count: u16,
    pub section_name_entry_index: u16,
}

#[repr(C, packed)]
pub struct SectionHeader {
    pub name_offset: u32,
    pub header_type: u32,
    pub header_flags: u32,
    pub section_address: u32,
    pub section_offset: u32,
    pub section_size: u32,
    pub linked_section: u32,
    pub info: u32,
    pub alignment: u32,
    pub fixed_entry_size: u32,
}
