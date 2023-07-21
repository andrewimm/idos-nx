use crate::memory::address::VirtualAddress;

use super::memory::SegmentedAddress;

/// The Program Segment Prefix (PSP) is an in-memory header that stores program
/// state. It is always paragraph-aligned. Many of the fields are unused legacy
/// values, but useful fields have been pubicly exposed.
#[repr(C, packed)]
pub struct PSP {
    /// Shortcut to terminate the program through INT 0x20
    int_20: [u8; 2], // 0x00
    /// First segment beyond the memory allocated to this program
    pub memory_top_paragraph: u16, // 0x02
    dos_reserved: u8, // 0x04
    /// Long jump to the DOS API dispatcher
    dispatcher_jump: [u8; 5], // 0x05
    /// Used to restore the value of INT 0x22, if changed by the program
    pub termination_vector: SegmentedAddress, // 0x0a
    /// Used to restore the value of INT 0x23, if changed by the program
    pub control_break_vector: SegmentedAddress, // 0x0e
    /// Used to restore the value of INT 0x24, if changed by the program
    pub critical_error_vector: SegmentedAddress, // 0x12
    /// Segment of the parent's PSP. If there is no parent, it is this PSP's
    /// own segment
    pub parent_segment: u16, // 0x16
    /// Job File Table: contains aliases from local handles to the
    /// corresponding entries in the DOS System File Table. The first five
    /// entries STDIN, STDOUT, STDERR, STDAUX, STDPRN
    pub file_handles: [u8; 20], // 0x18
    /// Segment of the current ENV string
    pub env_segment: u16, // 0x2c
    /// Stores the stack address when calling into the DOS API. Not really
    /// needed in our VM
    stack_save: SegmentedAddress, // 0x2e
    /// Length of the file handle table
    handle_array_length: u16, // 0x32
    /// Pointer to the file handle table, in case it has been relocated to fit
    /// beyond 20 files
    handle_array_pointer: SegmentedAddress, // 0x34
    /// Pointer to the previous PSP, but typically unused
    previous_psp: SegmentedAddress, // 0x38
    dos_reserved_2: [u8; 4], // 0x3c
    /// Contains the DOS version to return from API calls, in case it has been
    /// overridden with the SETVER command
    pub dos_version: u16, // 0x40
    dos_reserved_3: [u8; 14], // 0x42
    /// Another dispatcher: INT 0x21 + RETF
    dispatcher: [u8; 3], // 0x50
    unused: [u8; 9], // 0x53
    /// Reserved space for the first FCB
    fcb_first: [u8; 16], // 0x5c
    /// Reserved space for the second FCB
    fcb_second: [u8; 20], // 0x6c
    /// Number of bytes in the command tail
    pub command_tail_length: u8, // 0x80
    /// Actual contents of the command tail (the arguments passed after the
    /// executable command)
    pub command_tail: [u8; 127], // 0x81
}

impl PSP {
    pub fn reset(&mut self) {
        self.int_20 = [0xcd, 0x20];
        self.dispatcher = [0xcd, 0x21, 0xcb];
        self.parent_segment = self.get_segment();

        // TODO: implement env string
        self.env_segment = 0xe0;

        self.file_handles = [
            0, 1, 2, 3, 4,
            0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff,
        ];
    }

    pub fn as_ptr(&self) -> *const PSP {
        self as *const PSP
    }

    pub fn as_absolute_address(&self) -> VirtualAddress {
        VirtualAddress::new(self.as_ptr() as u32)
    }

    pub fn get_segment(&self) -> u16 {
        ((self.as_ptr() as u32) >> 4) as u16
    }

    pub fn as_segmented_address(&self) -> SegmentedAddress {
        SegmentedAddress {
            segment: self.get_segment(),
            offset: 0,
        }
    }

    pub unsafe fn at_segment(segment: u16) -> &'static mut PSP {
        let address = SegmentedAddress {
            segment,
            offset: 0,
        };
        let ptr = address.normalize().as_ptr_mut::<PSP>();
        &mut *ptr
    }

    pub fn get_parent_segment(&self) -> Option<u16> {
        if self.parent_segment == self.get_segment() {
            crate::kprintln!("PSP has no parent");
            return None;
        }
        Some(self.parent_segment)
    }

    pub fn find_empty_file_handle(&self) -> Option<usize> {
        let len = self.file_handles.len();
        for i in 0..len {
            if self.file_handles[i] == 0xff {
                return Some(i);
            }
        }
        None
    }
}

pub fn get_current_psp_segment() -> u16 {
    0x100
}

/// AH=0 - Terminate program
/// Restores the interrupt vectors 0x22, 0x23, 0x24. Frees memory allocated to
/// the current program, but does not close FCBs.
/// Input:
///      CS points to the PSP
pub fn legacy_terminate(cs: u16) -> SegmentedAddress {
    let psp = unsafe { PSP::at_segment(cs) };
    // TODO: reset vectors

    match psp.get_parent_segment() {
        Some(_) => {
            // jump to the parent through the termination vector
            psp.termination_vector
        },
        None => {
            // top-level program in the VM, terminate the Task
            crate::task::actions::lifecycle::terminate(0);
        },
    }
}

/// AH=0x4c - Terminate program
/// Return control to the calling program, storing an optional exit code to be
/// retrieved later.
/// Input:
///     AL = Exit code
pub fn terminate(code: u8) {
    // TODO: handle one program calling another

    crate::task::actions::lifecycle::terminate(code as u32);
}

