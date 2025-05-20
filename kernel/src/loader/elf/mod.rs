pub mod headers;
pub mod parse;

use alloc::vec::Vec;

use crate::{io::handle::Handle, memory::address::VirtualAddress};

use self::headers::{
    ProgramHeader, SECTION_FLAG_ALLOC, SECTION_TYPE_NOBITS, SEGMENT_FLAG_WRITE, SEGMENT_TYPE_LOAD,
};

use super::{
    environment::{ExecutionEnvironment, ExecutionSection, ExecutionSegment, InitialRegisters},
    error::LoaderError,
};

pub fn build_environment(exec_handle: Handle) -> Result<ExecutionEnvironment, LoaderError> {
    let elf_tables: parse::ElfTable = parse::load_tables(exec_handle)?;

    let mut segments: Vec<ExecutionSegment> = elf_tables
        .program_headers
        .iter()
        .map(|program_header: &ProgramHeader| {
            if program_header.segment_type != SEGMENT_TYPE_LOAD {
                return None;
            }
            let segment_start = program_header.virtual_address;
            let segment_end = program_header.virtual_address + program_header.memory_size;
            let address = segment_start.prev_page_barrier();
            let page_count = (segment_end.next_page_barrier() - address) / 0x1000;
            let mut segment = ExecutionSegment::at_address(address, page_count);
            segment.set_user_write_flag(program_header.flags & SEGMENT_FLAG_WRITE != 0);
            Some(segment)
        })
        .filter_map(|e| e)
        .collect();

    for section_header in &elf_tables.section_headers {
        // Only allocate memory for sections marked alloc
        if section_header.flags & SECTION_FLAG_ALLOC == 0 {
            continue;
        }
        let section_start = section_header.address;

        for segment in &mut segments {
            let segment_start = segment.get_starting_address();
            let segment_end = segment_start + segment.size_in_bytes();
            if segment_start <= section_start && segment_end > section_start {
                let offset = match section_header.section_type {
                    SECTION_TYPE_NOBITS => None,
                    _ => Some(section_header.offset),
                };
                let section = ExecutionSection {
                    segment_offset: section_start - segment_start,
                    size: section_header.file_size,
                    source_location: offset,
                };

                segment.add_section(section)?;
                break;
            }
        }
    }

    // create a segment for the stack
    let stack_size_pages = 2u32;
    let mut stack_segment = ExecutionSegment::at_address(
        VirtualAddress::new(0xc0000000 - stack_size_pages * 0x1000),
        stack_size_pages,
    );
    stack_segment.set_user_write_flag(true);
    let stack_section = ExecutionSection {
        segment_offset: 0,
        size: stack_size_pages * 0x1000,
        source_location: None,
    };
    stack_segment.add_section(stack_section)?;
    segments.push(stack_segment);

    let relocations = Vec::new();

    let environment = ExecutionEnvironment {
        segments,
        relocations,
        registers: InitialRegisters {
            eax: None,
            ebx: None,
            ecx: None,
            edx: None,
            ebp: None,
            esi: None,
            edi: None,
            eip: elf_tables.header.entry_point,
            esp: Some(0xc0000000),
            cs: None,
            ds: None,
            es: None,
            ss: None,
        },
        require_vm: false,
    };

    Ok(environment)
}
