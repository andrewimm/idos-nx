//! Parsing and loading for DOS COM files
//! Similar to raw binary files, there isn't any actual parsing or loading to
//! do. The file is mapped to a fixed location, and initial registers are set
//! up according to DOS convention.
//!
//! DOS processes run at a location beyond address 0 because they need to
//! preserve space for the initial PSP, as well as any DOS internals that need
//! to exist in memory.

use alloc::vec::Vec;

use crate::dos::execution::PSP;
use crate::filesystem::drive::DriveID;
use crate::files::handle::DriverHandle;
use crate::filesystem::get_driver_by_id;
use crate::memory::address::VirtualAddress;
use crate::task::memory::{ExecutionSegment, ExecutionSection};

use super::LoaderError;
use super::environment::{ExecutionEnvironment, InitialRegisters};


pub fn build_environment(drive: DriveID, driver_handle: DriverHandle) -> Result<ExecutionEnvironment, LoaderError> {
    let status = get_driver_by_id(drive)
        .map_err(|_| LoaderError::FileNotFound)?
        .stat(driver_handle)
        .map_err(|_| LoaderError::InternalError)?;

    // set the segment and IP
    let psp_segment: u32 = 0x100; // addresses will start at 0x1000
    let ip = core::mem::size_of::<PSP>() as u32;

    let segments = build_single_section_environment(status.byte_size, psp_segment)?;
    Ok(
        ExecutionEnvironment {
            segments,
            relocations: Vec::new(),
            registers: InitialRegisters {
                // eax is supposed to represent the validity of pre-constructed
                // FCBs. This needs to be implemented here
                eax: Some(0),
                ecx: Some(0),
                edx: Some(0),
                ebx: Some(0),
                ebp: Some(0),
                edi: Some(0),
                esi: Some(0),

                eip: ip,
                esp: Some(0xfffe),

                cs: Some(psp_segment),
                ds: Some(psp_segment),
                es: Some(psp_segment),
                ss: Some(psp_segment),
            },
            require_vm: true,
        }
    )
}

pub fn build_single_section_environment(file_size: u32, psp_segment: u32) -> Result<Vec<ExecutionSegment>, LoaderError> {
    let psp_start = psp_segment << 4;
    let psp_size = core::mem::size_of::<PSP>() as u32;
    let code_start = psp_start + psp_size;
    let page_start = VirtualAddress::new(psp_start).prev_page_barrier();

    let psp_section = ExecutionSection {
        segment_offset: psp_start - page_start.as_u32(),
        executable_file_offset: None,
        size: psp_size,
    };

    let code_section = ExecutionSection {
        segment_offset: code_start - page_start.as_u32(),
        executable_file_offset: Some(0),
        size: file_size,
    };

    let final_byte = code_start + file_size;
    let total_length = final_byte - page_start.as_u32();
    let mut page_count = total_length / 0x1000;
    if total_length & 0xfff != 0 {
        page_count += 1;
    }

    let mut segment = ExecutionSegment::at_address(page_start, page_count).map_err(|_| LoaderError::InternalError)?;
    segment.set_user_write_flag(true);
    segment.add_section(psp_section).map_err(|_| LoaderError::InternalError)?;
    segment.add_section(code_section).map_err(|_| LoaderError::InternalError)?;
    let mut segments = Vec::with_capacity(1);
    segments.push(segment);

    Ok(segments)
}

