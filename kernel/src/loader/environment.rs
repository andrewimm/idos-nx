use crate::{
    io::handle::Handle,
    memory::{
        address::{PhysicalAddress, VirtualAddress},
        physical::allocate_frame,
        virt::scratch::UnmappedPage,
    },
    task::{
        actions::{io::io_sync, memory::map_memory_for_task},
        id::TaskID,
        map::get_task,
        memory::MemoryBacking,
        paging::{ExternalPageDirectory, PermissionFlags},
    },
};
use alloc::vec::Vec;
use idos_api::io::ASYNC_OP_READ;

use super::{error::LoaderError, relocation::Relocation};

pub struct ExecutionEnvironment {
    pub registers: InitialRegisters,
    pub relocations: Vec<Relocation>,
    pub segments: Vec<ExecutionSegment>,
    pub require_vm: bool,
}

impl ExecutionEnvironment {
    pub fn map_memory(&mut self, task_id: TaskID) {
        for segment in &mut self.segments {
            segment.map_to_task(task_id);
        }

        let task_pagedir = get_task(task_id).unwrap().read().page_directory;
        // TODO: store mappings on the task itself, so they can be unmapped on termination

        super::LOGGER.log(format_args!(
            "Memory mapped for {:?}. Pagedir at {:?}",
            task_id, task_pagedir
        ));
    }

    pub fn fill_sections(&mut self, exec_handle: Handle) {
        // TODO: if sections were guaranteed to be sorted earlier, we wouldn't
        // be so inefficient!
        for segment in &self.segments {
            if segment.sections.is_empty() {
                continue;
            }
            for current_page in 0..segment.size_in_pages {
                let page_paddr = segment
                    .physical_frames
                    .get(current_page as usize)
                    .cloned()
                    .unwrap();
                let unmapped_page = UnmappedPage::map(page_paddr);

                let page_start_offset = current_page * 0x1000;
                let page_end_offset = page_start_offset + 0x1000;
                for section in &segment.sections {
                    let section_end_offset = section.segment_offset + section.size;
                    let overlap_start = page_start_offset.max(section.segment_offset);
                    let overlap_end = page_end_offset.min(section_end_offset);

                    if overlap_start >= overlap_end {
                        continue;
                    }
                    // overlap_start..overlap_end is the range of the section
                    // in the current page
                    match section.source_location {
                        Some(section_file_offset) => {
                            // in the first page of the section, we start reading from file_offset
                            let file_offset = if section.segment_offset >= page_start_offset {
                                section_file_offset
                            } else {
                                // in every other page of the section, we figure
                                // out how many pages deep we are, and then
                                // read from that same distance into the copy
                                // in the file
                                (section_file_offset & 0xffff_f000)
                                    + (page_start_offset - section.segment_offset + 0x1000)
                                    & 0xffff_f000
                            };
                            let relative_offset = overlap_start - page_start_offset;
                            let buffer_start = unmapped_page.virtual_address() + relative_offset;
                            let buffer_len = overlap_end - overlap_start;

                            super::LOGGER.log(format_args!(
                                "  \\ Load from {:#X} in file to {:?}",
                                file_offset, buffer_start
                            ));
                            let _ = io_sync(
                                exec_handle,
                                ASYNC_OP_READ,
                                buffer_start.as_u32(),
                                buffer_len,
                                file_offset,
                            );
                        }
                        None => {
                            // zero out contents?
                        }
                    }
                }
            }
        }
    }

    pub fn fill_stack(&self, task_id: TaskID) {
        // assume the last section of the last segment is the stack
        // if that's no longer the case, we need to change this
        let last_segment = self.segments.get(self.segments.len() - 1).unwrap();
        let last_frame = last_segment
            .physical_frames
            .get(last_segment.physical_frames.len() - 1)
            .cloned()
            .unwrap();

        let unmapped_stack_page = UnmappedPage::map(last_frame);
        let raw_stack_buffer: &mut [u8] = unsafe {
            core::slice::from_raw_parts_mut(
                unmapped_stack_page.virtual_address().as_ptr_mut::<u8>(),
                0x1000,
            )
        };
        let raw_stack_buffer_32: &mut [u32] = unsafe {
            core::slice::from_raw_parts_mut(
                unmapped_stack_page.virtual_address().as_ptr_mut::<u32>(),
                0x1000 / 4,
            )
        };

        let task_lock = get_task(task_id).unwrap();
        let task_guard = task_lock.read();
        let args = task_guard.args.arg_string();
        let mut args_start: usize = 0x1000 - args.len();
        // make sure that the start of the arg string is 4-bytes aligned,
        // so that all numbers below it on the stack are also aligned
        let alignment_offset = args_start & 3;
        if alignment_offset != 0 {
            args_start = args_start - alignment_offset;
        }
        // copy raw arg strings
        let args_slice = &mut raw_stack_buffer[args_start..(args_start + args.len())];
        args_slice.copy_from_slice(&args);
        // construct argv
        let arg_lengths = task_guard.args.arg_lengths();
        let arg_pointers_size = arg_lengths.len() * 4;
        let arg_pointers_start = args_start - arg_pointers_size;
        let mut arg_pointer_index = arg_pointers_start / 4;
        let mut string_offset = 0;
        for length in arg_lengths {
            raw_stack_buffer_32[arg_pointer_index] =
                0xbffff000 + (args_start as u32) + string_offset;
            string_offset += length;
            arg_pointer_index = arg_pointer_index + 1;
        }
        // construct argc
        let arg_count_index = arg_pointers_start / 4 - 1;
        raw_stack_buffer_32[arg_count_index] = task_guard.args.arg_count();
    }

    pub fn set_registers(&self, task_id: TaskID) {
        let flags = 0;

        let task_lock = get_task(task_id).unwrap();
        let esp_start = self.registers.esp.unwrap_or(0xc000_0000);
        let esp = {
            let task_guard = task_lock.read();
            esp_start - task_guard.args.stack_size() as u32
        };

        let task_lock = get_task(task_id).unwrap();
        let mut task_guard = task_lock.write();

        // TODO: GS, FS, ES, DS aren't actually popped. This is wrong.
        task_guard.stack_push_u32(0); // GS
        task_guard.stack_push_u32(self.registers.fs.unwrap_or(0));
        task_guard.stack_push_u32(self.registers.es.unwrap_or(0x20 | 3));
        task_guard.stack_push_u32(self.registers.ds.unwrap_or(0x20 | 3));
        task_guard.stack_push_u32(self.registers.ss.unwrap_or(0x20 | 3));
        task_guard.stack_push_u32(esp);
        task_guard.stack_push_u32(flags);
        task_guard.stack_push_u32(self.registers.cs.unwrap_or(0x18 | 3));
        task_guard.stack_push_u32(self.registers.eip);
        task_guard.stack_push_u32(self.registers.edi.unwrap_or(0));
        task_guard.stack_push_u32(self.registers.esi.unwrap_or(0));
        task_guard.stack_push_u32(self.registers.ebp.unwrap_or(0));
        task_guard.stack_push_u32(self.registers.ebx.unwrap_or(0));
        task_guard.stack_push_u32(self.registers.edx.unwrap_or(0));
        task_guard.stack_push_u32(self.registers.ecx.unwrap_or(0));
        task_guard.stack_push_u32(self.registers.eax.unwrap_or(0));
    }
}

#[derive(Default)]
pub struct InitialRegisters {
    pub eax: Option<u32>,
    pub ecx: Option<u32>,
    pub edx: Option<u32>,
    pub ebx: Option<u32>,
    pub ebp: Option<u32>,
    pub esi: Option<u32>,
    pub edi: Option<u32>,

    pub eip: u32,
    pub esp: Option<u32>,

    pub cs: Option<u32>,
    pub ds: Option<u32>,
    pub es: Option<u32>,
    pub fs: Option<u32>,
    pub ss: Option<u32>,
}

pub struct ExecutionSegment {
    /// Initialize location of the segment, must be page aligned
    start_address: VirtualAddress,
    /// Size of the segment, in pages
    size_in_pages: u32,
    /// All of the sections found in the segment
    sections: Vec<ExecutionSection>,
    /// Flag to determine whether the backing pages should be userspace writable
    user_can_write: bool,
    /// List of physical addresses for the backing frames. After the segment is
    /// extracted, frames of memory will be allocated for the segment, and
    /// those are stored here so that the loader Task can map those frames and
    /// fill them with disk contents or apply relocations.
    physical_frames: Vec<PhysicalAddress>,
}

impl ExecutionSegment {
    pub fn at_address(start_address: VirtualAddress, size_in_pages: u32) -> Self {
        Self {
            start_address,
            size_in_pages,
            sections: Vec::new(),
            user_can_write: false,
            physical_frames: Vec::new(),
        }
    }

    pub fn get_starting_address(&self) -> VirtualAddress {
        self.start_address
    }

    pub fn can_write(&self) -> bool {
        self.user_can_write
    }

    pub fn set_user_write_flag(&mut self, flag: bool) {
        self.user_can_write = flag;
    }

    pub fn size_in_bytes(&self) -> u32 {
        self.size_in_pages * 0x1000
    }

    pub fn add_section(&mut self, section: ExecutionSection) -> Result<(), LoaderError> {
        if section.segment_offset + section.size > self.size_in_bytes() {
            return Err(LoaderError::SectionOutOfBounds);
        }
        self.sections.push(section);
        Ok(())
    }

    pub fn map_to_task(&mut self, task_id: TaskID) {
        if !self.physical_frames.is_empty() {
            panic!("Exec segment already mapped");
        }
        map_memory_for_task(
            task_id,
            Some(self.start_address),
            self.size_in_bytes(),
            MemoryBacking::Anonymous,
        )
        .unwrap();

        let external_dir = ExternalPageDirectory::for_task(task_id);
        for page in 0..self.size_in_pages {
            let frame = allocate_frame().unwrap();
            let frame_paddr = frame.to_physical_address();
            self.physical_frames.push(frame_paddr);
            let page_vaddr = self.start_address + (0x1000 * page);
            let mut flags = PermissionFlags::USER_ACCESS;
            if self.user_can_write {
                flags |= PermissionFlags::WRITE_ACCESS;
            }
            external_dir.map(page_vaddr, frame_paddr, PermissionFlags::new(flags));
        }
    }
}

pub struct ExecutionSection {
    /// Start location of this section within the parent segment
    pub segment_offset: u32,
    /// Size in bytes of the section
    pub size: u32,
    /// Location of this section's data within the executable file, if it's
    /// backed by program bytes. If it should be zeroed out, this value is
    /// None.
    pub source_location: Option<u32>,
}
