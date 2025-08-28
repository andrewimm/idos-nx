use idos_api::io::error::IOError;

use crate::{
    memory::address::VirtualAddress,
    task::{id::TaskID, messaging::Message},
};

pub type IOResult = Result<u32, IOError>;

#[derive(Copy, Clone, Debug)]
pub enum DriverIOAction {
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
}

impl DriverIOAction {
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
        }
    }
}

#[repr(u32)]
pub enum DriverCommand {
    Open = 1,
    OpenRaw,
    Read,
    Write,
    Close,
    Stat,
    Share,
    Ioctl,
    // Every time a new command is added, modify the method below that decodes the command
    Invalid = 0xffffffff,
}

impl DriverCommand {
    pub fn from_u32(code: u32) -> DriverCommand {
        match code {
            1 => DriverCommand::Open,
            2 => DriverCommand::OpenRaw,
            3 => DriverCommand::Read,
            4 => DriverCommand::Write,
            5 => DriverCommand::Close,
            6 => DriverCommand::Stat,
            7 => DriverCommand::Share,
            8 => DriverCommand::Ioctl,
            _ => DriverCommand::Invalid,
        }
    }
}
