use alloc::vec::Vec;
use crate::files::cursor::SeekMethod;
use crate::files::handle::DriverHandle;
use crate::filesystem::get_driver_by_id;
use crate::filesystem::drive::DriveID;
use crate::loader::LoaderError;

use super::headers::{ElfHeader, ProgramHeader, SectionHeader, FileHeader};

pub fn load_tables(drive_id: DriveID, driver_handle: DriverHandle) -> Result<(ElfHeader, Vec<ProgramHeader>, Vec<SectionHeader>), LoaderError> {
    let driver = get_driver_by_id(drive_id)
        .map_err(|_| LoaderError::FileNotFound)?;

    let mut header: ElfHeader = unsafe { core::mem::zeroed::<ElfHeader>() };
    driver.seek(driver_handle, SeekMethod::Absolute(0)).map_err(|_| LoaderError::FileNotFound)?;
    driver.read(driver_handle, header.as_buffer_mut()).map_err(|_| LoaderError::FileNotFound)?;

    let mut program_table: Vec<ProgramHeader> = Vec::with_capacity(header.program_header_table_count as usize);
    let mut section_table: Vec<SectionHeader> = Vec::with_capacity(header.section_header_table_count as usize);

    driver.seek(driver_handle, SeekMethod::Absolute(header.program_header_table_offset as usize)).map_err(|_| LoaderError::FileNotFound)?;
    for _ in 0..header.program_header_table_count {
        let mut entry: ProgramHeader = unsafe { core::mem::zeroed::<ProgramHeader>() };
        driver.read(driver_handle, entry.as_buffer_mut()).map_err(|_| LoaderError::FileNotFound)?;
        program_table.push(entry);
    }

    driver.seek(driver_handle, SeekMethod::Absolute(header.section_header_table_offset as usize)).map_err(|_| LoaderError::FileNotFound)?;
    for _ in 0..header.section_header_table_count {
        let mut entry: SectionHeader = unsafe { core::mem::zeroed::<SectionHeader>() };
        driver.read(driver_handle, entry.as_buffer_mut()).map_err(|_| LoaderError::FileNotFound)?;
        section_table.push(entry);
    }

    Ok((header, program_table, section_table))
}
