use idos_api::io::driver::DriverCommand;
use idos_api::ipc::Message;

use crate::{
    memory::address::{PhysicalAddress, VirtualAddress},
    task::id::TaskID,
};

#[derive(Copy, Clone, Debug)]
pub enum DriverIoAction {
    /// Open an absolute path string
    Open {
        path_str_vaddr: VirtualAddress,
        path_str_len: usize,
    },
    /// Open a handle to the driver itself, with no path.
    /// The argument provides a way to embed a unique instance identifier
    /// without using a string path -- this is commonly used by async device
    /// drivers which run multiple instances from a single Task.
    OpenRaw { driver_id: u32 },
    /// Close an open file instance
    Close { instance: u32 },
    /// Read an open file instance, providing the location and size of a buffer
    /// to copy data into, and an offset to start reading from.
    Read {
        instance: u32,
        buffer_ptr_vaddr: VirtualAddress,
        buffer_len: usize,
        starting_offset: u32,
    },
    /// Write an open file instance, providing the location and size of a buffer
    /// containing the data to write, and an offset location to write to.
    Write {
        instance: u32,
        buffer_ptr_vaddr: VirtualAddress,
        buffer_len: usize,
        starting_offset: u32,
    },
    /// Stat an open file instance, providing the location and size of a
    /// writable stat object
    Stat {
        instance: u32,
        stat_ptr_vaddr: VirtualAddress,
        stat_len: usize,
    },
    /// Share an open file instance with another task.
    Share {
        instance: u32,
        dest_task_id: TaskID,
        is_move: bool,
    },
    /// General IOCTL
    Ioctl { instance: u32, ioctl: u32, arg: u32 },
    /// IOCTL with struct arg
    IoctlStruct {
        instance: u32,
        ioctl: u32,
        arg_ptr_vaddr: VirtualAddress,
        arg_len: usize,
    },
    /// Map a file into memory
    CreateFileMapping {
        path_str_vaddr: VirtualAddress,
        path_str_len: usize,
    },
    /// Unmap a file mapping
    RemoveFileMapping { mapping_token: u32 },
    /// Create a directory at a path
    Mkdir {
        path_str_vaddr: VirtualAddress,
        path_str_len: usize,
    },
    /// Remove (unlink) a file at a path
    Unlink {
        path_str_vaddr: VirtualAddress,
        path_str_len: usize,
    },
    /// Remove an empty directory at a path
    Rmdir {
        path_str_vaddr: VirtualAddress,
        path_str_len: usize,
    },
    /// Copy file contents into memory frame
    PageInFileMapping {
        mapping_token: u32,
        offset_in_file: u32,
        frame_paddr: PhysicalAddress,
    },
}

impl DriverIoAction {
    pub fn encode_to_message(&self, request_id: u32) -> Message {
        match self {
            Self::Open {
                path_str_vaddr,
                path_str_len,
            } => Message {
                message_type: DriverCommand::Open as u32,
                unique_id: request_id,
                args: [path_str_vaddr.as_u32(), *path_str_len as u32, 0, 0, 0, 0],
            },
            Self::OpenRaw { driver_id } => Message {
                message_type: DriverCommand::OpenRaw as u32,
                unique_id: request_id,
                args: [*driver_id, 0, 0, 0, 0, 0],
            },
            Self::Close { instance } => Message {
                message_type: DriverCommand::Close as u32,
                unique_id: request_id,
                args: [*instance, 0, 0, 0, 0, 0],
            },
            Self::Read {
                instance,
                buffer_ptr_vaddr,
                buffer_len,
                starting_offset,
            } => Message {
                message_type: DriverCommand::Read as u32,
                unique_id: request_id,
                args: [
                    *instance,
                    buffer_ptr_vaddr.as_u32(),
                    *buffer_len as u32,
                    *starting_offset,
                    0,
                    0,
                ],
            },
            Self::Write {
                instance,
                buffer_ptr_vaddr,
                buffer_len,
                starting_offset,
            } => Message {
                message_type: DriverCommand::Write as u32,
                unique_id: request_id,
                args: [
                    *instance,
                    buffer_ptr_vaddr.as_u32(),
                    *buffer_len as u32,
                    *starting_offset,
                    0,
                    0,
                ],
            },
            Self::Stat {
                instance,
                stat_ptr_vaddr,
                stat_len,
            } => Message {
                message_type: DriverCommand::Stat as u32,
                unique_id: request_id,
                args: [
                    *instance,
                    stat_ptr_vaddr.as_u32(),
                    *stat_len as u32,
                    0,
                    0,
                    0,
                ],
            },
            Self::Share {
                instance,
                dest_task_id,
                is_move,
            } => Message {
                message_type: DriverCommand::Share as u32,
                unique_id: request_id,
                args: [
                    *instance,
                    (*dest_task_id).into(),
                    if *is_move { 1 } else { 0 },
                    0,
                    0,
                    0,
                ],
            },
            Self::Ioctl {
                instance,
                ioctl,
                arg,
            } => Message {
                message_type: DriverCommand::Ioctl as u32,
                unique_id: request_id,
                args: [*instance, *ioctl, *arg, 0, 0, 0],
            },
            Self::IoctlStruct {
                instance,
                ioctl,
                arg_ptr_vaddr,
                arg_len,
            } => Message {
                message_type: DriverCommand::Ioctl as u32,
                unique_id: request_id,
                args: [
                    *instance,
                    *ioctl,
                    arg_ptr_vaddr.as_u32(),
                    *arg_len as u32,
                    0,
                    0,
                ],
            },
            Self::Mkdir {
                path_str_vaddr,
                path_str_len,
            } => Message {
                message_type: DriverCommand::Mkdir as u32,
                unique_id: request_id,
                args: [path_str_vaddr.as_u32(), *path_str_len as u32, 0, 0, 0, 0],
            },
            Self::Unlink {
                path_str_vaddr,
                path_str_len,
            } => Message {
                message_type: DriverCommand::Unlink as u32,
                unique_id: request_id,
                args: [path_str_vaddr.as_u32(), *path_str_len as u32, 0, 0, 0, 0],
            },
            Self::Rmdir {
                path_str_vaddr,
                path_str_len,
            } => Message {
                message_type: DriverCommand::Rmdir as u32,
                unique_id: request_id,
                args: [path_str_vaddr.as_u32(), *path_str_len as u32, 0, 0, 0, 0],
            },
            Self::CreateFileMapping {
                path_str_vaddr,
                path_str_len,
            } => Message {
                message_type: DriverCommand::CreateMapping as u32,
                unique_id: request_id,
                args: [path_str_vaddr.as_u32(), *path_str_len as u32, 0, 0, 0, 0],
            },
            Self::RemoveFileMapping { mapping_token } => Message {
                message_type: DriverCommand::RemoveMapping as u32,
                unique_id: request_id,
                args: [*mapping_token, 0, 0, 0, 0, 0],
            },
            Self::PageInFileMapping {
                mapping_token,
                offset_in_file,
                frame_paddr,
            } => Message {
                message_type: DriverCommand::PageInMapping as u32,
                unique_id: request_id,
                args: [
                    *mapping_token,
                    *offset_in_file,
                    frame_paddr.as_u32(),
                    0,
                    0,
                    0,
                ],
            },
        }
    }
}
