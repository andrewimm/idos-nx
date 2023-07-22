use alloc::vec::Vec;

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

    let segments = build_single_section_environment(status.byte_size, 0)?;
    Ok(
        ExecutionEnvironment {
            segments,
            relocations: Vec::new(),
            registers: InitialRegisters {
                eax: Some(0xaa),
                ecx: Some(0xcc),
                edx: Some(0xdd),
                ebx: Some(0xbb),
                ebp: Some(0xb0),
                edi: Some(0xd1),
                esi: Some(0x51),

                eip: 0,
                esp: Some(0xc0000000),

                cs: None,
                ds: None,
                es: None,
                ss: None,
            },
            require_vm: false,
        }
    )
}

pub fn build_single_section_environment(file_size: u32, offset: u32) -> Result<Vec<ExecutionSegment>, LoaderError> {
    let data_size = file_size + offset;
    let section = ExecutionSection {
        segment_offset: offset,
        executable_file_offset: Some(0),
        size: data_size,
    };
    let mut page_count = data_size / 0x1000;
    if data_size & 0xfff != 0 {
        page_count += 1;
    }
    let address = VirtualAddress::new(0);
    let mut segment = ExecutionSegment::at_address(address, page_count).map_err(|_| LoaderError::InternalError)?;
    segment.set_user_write_flag(true);
    segment.add_section(section).map_err(|_| LoaderError::InternalError)?;
    let mut segments = Vec::with_capacity(1);
    segments.push(segment);
    Ok(segments)
}
