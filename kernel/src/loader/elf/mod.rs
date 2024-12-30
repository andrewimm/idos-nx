use crate::files::handle::DriverHandle;
use crate::filesystem::drive::DriveID;
use crate::loader::elf::headers::ProgramHeader;
use crate::memory::address::VirtualAddress;
use crate::task::memory::{ExecutionSection, ExecutionSegment};
use alloc::vec::Vec;

use super::environment::{ExecutionEnvironment, InitialRegisters};
use super::LoaderError;

pub mod headers;
pub mod parse;

pub fn build_environment(
    drive_id: DriveID,
    driver_handle: DriverHandle,
) -> Result<ExecutionEnvironment, LoaderError> {
    let (header, program_headers, section_headers) = parse::load_tables(drive_id, driver_handle)?;

    let mut segments: Vec<ExecutionSegment> = program_headers
        .iter()
        .map(|program_header: &ProgramHeader| {
            if program_header.segment_type != headers::SEGMENT_TYPE_LOAD {
                return None;
            }
            let segment_start = program_header.segment_virtual_address;
            let segment_end = segment_start + program_header.segment_size_in_memory;
            let address = segment_start.prev_page_barrier();
            let page_count = (segment_end.next_page_barrier() - address) / 4096;
            let mut segment = ExecutionSegment::at_address(address, page_count).ok()?;
            segment.set_user_write_flag(
                program_header.segment_flags & headers::SEGMENT_FLAG_WRITE != 0,
            );

            Some(segment)
        })
        .filter_map(|e| e)
        .collect();

    for section_header in section_headers.iter() {
        // only allocate memory for sections marked ALLOC
        if section_header.section_flags & headers::SECTION_FLAG_ALLOC == 0 {
            continue;
        }
        let section_start = section_header.section_virtual_address;

        for segment in segments.iter_mut() {
            let segment_start = segment.get_starting_address();
            let segment_end = segment_start + segment.size_in_bytes();
            if (segment_start..segment_end).contains(&section_start) {
                let offset = match section_header.section_type {
                    headers::SECTION_TYPE_NOBITS => None,
                    _ => Some(section_header.section_file_offset),
                };
                let section = ExecutionSection {
                    segment_offset: section_start - segment_start,
                    executable_file_offset: offset,
                    size: section_header.section_size_in_file,
                };

                segment
                    .add_section(section)
                    .map_err(|_| LoaderError::InternalError)?;
                break;
            }
        }
    }

    // create a segment for the stack
    let stack_size_pages: u32 = 2;
    let mut stack_segment = ExecutionSegment::at_address(
        VirtualAddress::new(0xc0000000 - 0x1000 * stack_size_pages),
        stack_size_pages,
    )
    .map_err(|_| LoaderError::InternalError)?;
    stack_segment.set_user_write_flag(true);
    let stack_section = ExecutionSection {
        segment_offset: 0,
        executable_file_offset: None,
        size: stack_size_pages * 0x1000,
    };
    stack_segment
        .add_section(stack_section)
        .map_err(|_| LoaderError::InternalError)?;
    segments.push(stack_segment);

    let relocations = Vec::new();

    let env = ExecutionEnvironment {
        segments,
        relocations,
        registers: InitialRegisters {
            eax: None,
            ecx: None,
            edx: None,
            ebx: None,
            ebp: None,
            edi: None,
            esi: None,
            eip: header.entry_point,
            esp: Some(0xc0000000),
            cs: None,
            ds: None,
            es: None,
            ss: None,
        },
        require_vm: false,
    };

    Ok(env)
}
