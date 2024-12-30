use alloc::vec::Vec;

use super::environment::{ExecutionEnvironment, InitialRegisters};
use super::parse::FileHeader;
use super::relocation::Relocation;
use super::LoaderError;
use crate::dos::execution::PSP;
use crate::dos::memory::SegmentedAddress;
use crate::files::cursor::SeekMethod;
use crate::files::handle::DriverHandle;
use crate::filesystem::drive::DriveID;
use crate::filesystem::get_driver_by_id;
use crate::memory::address::VirtualAddress;
use crate::task::memory::{ExecutionSection, ExecutionSegment};

#[repr(C, packed)]
#[allow(dead_code)]
pub struct MzHeader {
    magic_number: [u8; 2],
    /// Number of bytes actually occupied in the final page
    last_page_bytes: u16,
    /// Number of 512B "pages" needed to contain this file
    page_count: u16,
    /// Number of entries in the relocation table
    relocation_entries: u16,
    /// Size of this header, in paragraphs (4 bytes)
    header_size_paragraphs: u16,
    /// Minimum number of paragraphs needed for execution. This is used for
    /// uninitialized data that appears
    min_alloc_paragraphs: u16,
    /// Maximum number of paragraphs needed for execution. This is the amount
    /// preferred by the program
    max_alloc_paragraphs: u16,
    /// Initial value of the SS segment, offset from the program's PSP segment
    initial_ss: u16,
    /// Initial value of the SP register
    initial_sp: u16,
    /// Data integrity checksum
    checksum: u16,
    /// Initial value of the IP register
    initial_ip: u16,
    /// Initial value of the CS segment, offset from the program's PSP segment
    initial_cs: u16,
    /// Location of the relocation table, relative to file start
    relocation_table_offset: u16,
    /// Overlay number
    overlay_number: u16,
}

impl MzHeader {
    pub fn byte_length(&self) -> usize {
        if self.page_count == 0 {
            return 0;
        }
        let mut size = (self.page_count as usize - 1) * 512;
        size += if self.last_page_bytes == 0 {
            512
        } else {
            self.last_page_bytes as usize
        };
        size
    }

    pub fn header_size_bytes(&self) -> usize {
        (self.header_size_paragraphs as usize) << 4
    }
}

impl FileHeader for MzHeader {}

fn read_exec_file(
    drive_id: DriveID,
    driver_handle: DriverHandle,
    seek: usize,
    buffer: &mut [u8],
) -> Result<u32, LoaderError> {
    let driver = get_driver_by_id(drive_id).map_err(|_| LoaderError::FileNotFound)?;
    driver
        .seek(driver_handle, SeekMethod::Absolute(seek))
        .map_err(|_| LoaderError::FileNotFound)?;
    driver
        .read(driver_handle, buffer)
        .map_err(|_| LoaderError::FileNotFound)
}

pub fn build_environment(
    drive_id: DriveID,
    driver_handle: DriverHandle,
) -> Result<ExecutionEnvironment, LoaderError> {
    let mut mz_header = unsafe { core::mem::zeroed::<MzHeader>() };
    read_exec_file(drive_id, driver_handle, 0, mz_header.as_buffer_mut())?;
    if mz_header.page_count < 1 {
        return Err(LoaderError::InternalError);
    }

    let file_size = mz_header.byte_length() as u32;
    let mz_header_size = mz_header.header_size_bytes() as u32;
    let program_size = file_size - mz_header_size;

    let psp_segment: u32 = 0x100;

    // The "load module" is the code copied from the EXE, at a new segment
    // after the psp
    let load_module_segment = psp_segment + (core::mem::size_of::<PSP>() as u32 >> 4);

    let segments = {
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
            executable_file_offset: Some(mz_header_size),
            size: program_size,
        };

        let final_byte = code_start + file_size;
        let total_length = final_byte - page_start.as_u32();
        let mut page_count = total_length / 0x1000;
        if total_length & 0xfff != 0 {
            page_count += 1;
        }

        let mut segment = ExecutionSegment::at_address(page_start, page_count)
            .map_err(|_| LoaderError::InternalError)?;
        segment.set_user_write_flag(true);
        segment
            .add_section(psp_section)
            .map_err(|_| LoaderError::InternalError)?;
        segment
            .add_section(code_section)
            .map_err(|_| LoaderError::InternalError)?;
        let mut segments = Vec::with_capacity(1);
        segments.push(segment);
        segments
    };

    let relocations: Vec<Relocation> = {
        let relocation_table_size = mz_header.relocation_entries as usize;
        let mut relocation_table: Vec<SegmentedAddress> = Vec::with_capacity(relocation_table_size);
        for _ in 0..relocation_table_size {
            relocation_table.push(SegmentedAddress {
                segment: 0,
                offset: 0,
            });
        }
        let relocation_table_bytes = unsafe {
            core::slice::from_raw_parts_mut(
                relocation_table.as_mut_ptr() as *mut u8,
                relocation_table.len() * core::mem::size_of::<SegmentedAddress>(),
            )
        };
        read_exec_file(
            drive_id,
            driver_handle,
            mz_header.relocation_table_offset as usize,
            relocation_table_bytes,
        )?;

        relocation_table
            .iter()
            .map(|seg| {
                let addr = seg.normalize() + (load_module_segment << 4);
                Relocation::DosExe(addr, load_module_segment as u16)
            })
            .collect()
    };

    crate::kprintln!("Relocations: {:?}", relocations);

    Ok(ExecutionEnvironment {
        segments,
        relocations,
        registers: InitialRegisters {
            // Similar to COM, this should represent validity of FCBs in
            // the PSP
            eax: Some(0),
            ebx: None,
            ecx: None,
            edx: None,
            ebp: None,
            esi: None,
            edi: None,

            eip: mz_header.initial_ip as u32,
            esp: Some(mz_header.initial_sp as u32),

            cs: Some(mz_header.initial_cs as u32 + load_module_segment),
            ds: Some(psp_segment as u32),
            es: Some(psp_segment as u32),
            ss: Some(mz_header.initial_ss as u32 + load_module_segment),
        },
        require_vm: true,
    })
}
