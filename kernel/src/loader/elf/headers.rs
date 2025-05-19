//! All of the headers from the ELF file format

use crate::memory::address::VirtualAddress;

use super::super::parse::FileHeader;

#[derive(Default)]
#[repr(C, packed)]
pub struct ElfHeader {
    pub magic: [u8; 4],
    /// 1 indicates 32-bit, 2 indicates 64-bit
    pub bit_class: u8,
    /// 1 indicates little-endian, 2 indicates big-endian
    pub endianness: u8,
    pub id_version: u8,
    /// Target OS ABI, usually set to zero
    pub os_abi: u8,
    pub abi_version: u8,
    pub _padding: [u8; 7],
    /// Determines how the file should be interpreted
    pub object_file_type: u16,
    /// Specifies the target machine arch
    pub machine: u16,
    pub elf_version: u32,
    /// Address of entry point
    pub entry_point: u32,
    /// Pointer to the program header table, as an offset from the start of the file
    pub program_header_offset: u32,
    /// Pointer to the section header table, as an offset from the start of the file
    pub section_header_offset: u32,
    pub flags: u32,
    /// Size of this header, typically 52 bytes
    pub header_size: u16,
    /// Size of each entry in the program header table
    pub program_header_size: u16,
    /// Number of entries in the program header table
    pub program_header_count: u16,
    /// Size of each entry in the section header table
    pub section_header_size: u16,
    /// Number of entries in the section header table
    pub section_header_count: u16,
    /// Index of the section header that contains the section names
    pub section_name_index: u16,
}

impl FileHeader for ElfHeader {}

#[derive(Default)]
#[repr(C, packed)]
pub struct ProgramHeader {
    /// Type of segment. Use the SEGMENT_TYPE_* constants to interpret
    pub segment_type: u32,
    /// Offset of the segment in the file
    pub offset: u32,
    /// Virtual address of the segment in memory
    pub virtual_address: VirtualAddress,
    /// Physical address of the segment in memory, only relevant on certain systems
    pub physical_address: u32,
    /// Size of the segment in the file
    pub file_size: u32,
    /// Size of the segment in memory
    pub memory_size: u32,
    /// Flags for the segment
    pub flags: u32,
    /// Alignment of the segment
    pub alignment: u32,
}

impl FileHeader for ProgramHeader {}

pub const SEGMENT_TYPE_NULL: u32 = 0;
pub const SEGMENT_TYPE_LOAD: u32 = 1;
pub const SEGMENT_TYPE_DYNAMIC: u32 = 2;

pub const SEGMENT_FLAG_EXEC: u32 = 1 << 0;
pub const SEGMENT_FLAG_WRITE: u32 = 1 << 1;
pub const SEGMENT_FLAG_READ: u32 = 1 << 2;

#[derive(Default)]
#[repr(C, packed)]
pub struct SectionHeader {
    /// Offset of the section name in the string table
    pub name: u32,
    /// Type of the section. Use the SECTION_TYPE_* constants to interpret
    pub section_type: u32,
    /// Flags for the section
    pub flags: u32,
    /// Address of the section in memory
    pub address: VirtualAddress,
    /// Offset of the section in the file
    pub offset: u32,
    /// Size of the section in the file
    pub file_size: u32,
    /// Index of an associated section, if applicable
    pub link: u32,
    /// Extra information, depending on section type
    pub info: u32,
    /// Address alignment requirement, as a power of 2
    pub address_alignment: u32,
    /// Size of each entry in the section, if section type contains a table of fixed-size entries
    pub entry_size: u32,
}

impl FileHeader for SectionHeader {}

/// Inactive section header; the first entry is always null
pub const SECTION_TYPE_NULL: u32 = 0;
/// Bits defined by the program, which have meaning during load / interpretation
pub const SECTION_TYPE_PROGBITS: u32 = 1;
/// Symbol table
pub const SECTION_TYPE_SYMTAB: u32 = 2;
/// String table
pub const SECTION_TYPE_STRTAB: u32 = 3;
/// Relocation entries with known addends
pub const SECTION_TYPE_RELA: u32 = 4;
/// Hash table for symbol lookup
pub const SECTION_TYPE_HASH: u32 = 5;
/// Information needed for dynamic linking
pub const SECTION_TYPE_DYNAMIC: u32 = 6;
/// Markers specific to the file contents
pub const SECTION_TYPE_NOTE: u32 = 7;
/// Not backed by any file contents
pub const SECTION_TYPE_NOBITS: u32 = 8;
/// Relocation entries without addends
pub const SECTION_TYPE_REL: u32 = 9;

pub const SECTION_FLAG_WRITE: u32 = 1 << 0;
pub const SECTION_FLAG_ALLOC: u32 = 1 << 1;
pub const SECTION_FLAG_EXEC: u32 = 1 << 2;
pub const SECTION_FLAG_MERGE: u32 = 1 << 4;
pub const SECTION_FLAG_STRINGS: u32 = 1 << 5;
