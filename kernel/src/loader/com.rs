use alloc::vec::Vec;
use idos_api::io::{file::FileStatus, FILE_OP_STAT};

use crate::{io::handle::Handle, memory::address::VirtualAddress, task::actions::io::io_sync};

use super::{
    environment::{ExecutionEnvironment, ExecutionSection, ExecutionSegment, InitialRegisters},
    error::LoaderError,
};

pub fn build_environment(exec_handle: Handle) -> Result<ExecutionEnvironment, LoaderError> {
    let mut file_status = FileStatus::new();
    let file_status_ptr = &mut file_status as *mut FileStatus;
    let _ = io_sync(
        exec_handle,
        FILE_OP_STAT,
        file_status_ptr as u32,
        core::mem::size_of::<FileStatus>() as u32,
        0,
    )
    .map_err(|_| LoaderError::InternalError)?;

    let section = ExecutionSection {
        segment_offset: 0,
        size: file_status.byte_size,
        source_location: Some(0),
    };

    let page_count = (file_status.byte_size + 0xfff) / 0x1000;
    let mut segment = ExecutionSegment::at_address(VirtualAddress::new(0x8000), page_count);
    segment.set_user_write_flag(true);
    segment
        .add_section(section)
        .map_err(|_| LoaderError::InternalError)?;

    let mut segments = Vec::new();
    segments.push(segment);
    let relocations = Vec::new();

    let environment = ExecutionEnvironment {
        segments,
        relocations,
        registers: InitialRegisters::default(),
        require_vm: true,
    };

    Ok(environment)
}
