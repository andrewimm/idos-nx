use crate::{
    memory::{
        address::{PhysicalAddress, VirtualAddress},
        physical::allocate_frame,
    },
    task::{
        actions::memory::map_memory_for_task,
        id::TaskID,
        memory::MemoryBacking,
        paging::{ExternalPageDirectory, PermissionFlags},
        switching::get_task,
    },
};
use alloc::vec::Vec;

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
        crate::kprintln!("LOADER - Memory Mapped. Pagedir at {:?}", task_pagedir);
    }
}

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
    /// Flag to determine whether the backing pages should be userspace executable
    user_can_exec: bool,
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
            user_can_exec: false,
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
        );

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
