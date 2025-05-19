use alloc::vec::Vec;

use crate::{io::handle::Handle, loader::error::LoaderError, task::actions::io::read_struct_sync};

use super::headers::{ElfHeader, ProgramHeader, SectionHeader};

pub struct ElfTable {
    pub header: ElfHeader,
    pub program_headers: Vec<ProgramHeader>,
    pub section_headers: Vec<SectionHeader>,
}

pub fn load_tables(exec_handle: Handle) -> Result<ElfTable, LoaderError> {
    let mut elf_header: ElfHeader = ElfHeader::default();
    let _ = read_struct_sync(exec_handle, &mut elf_header, 0)
        .map_err(|_| LoaderError::InternalError)?;

    let mut program_headers: Vec<ProgramHeader> =
        Vec::with_capacity(elf_header.program_header_count as usize);
    let mut section_headers: Vec<SectionHeader> =
        Vec::with_capacity(elf_header.section_header_count as usize);

    let mut read_offset = elf_header.program_header_offset;
    for _ in 0..elf_header.program_header_count {
        let mut program_header: ProgramHeader = ProgramHeader::default();
        let _ = read_struct_sync(exec_handle, &mut program_header, read_offset)
            .map_err(|_| LoaderError::InternalError)?;
        program_headers.push(program_header);
        read_offset += elf_header.program_header_size as u32;
    }

    read_offset = elf_header.section_header_offset;
    for _ in 0..elf_header.section_header_count {
        let mut section_header: SectionHeader = SectionHeader::default();
        let _ = read_struct_sync(exec_handle, &mut section_header, read_offset)
            .map_err(|_| LoaderError::InternalError)?;
        section_headers.push(section_header);
        read_offset += elf_header.section_header_size as u32;
    }

    Ok(ElfTable {
        header: elf_header,
        program_headers,
        section_headers,
    })
}
