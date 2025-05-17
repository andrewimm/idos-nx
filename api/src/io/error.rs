#[derive(Debug, PartialEq)]
#[repr(u32)]
pub enum IOError {
    // No enum value should be backed by a value of 0
    /// An error occurred within the file system
    FileSystemError = 1,
    /// A File or Directory with the given path does not exist
    NotFound,
    /// The file handle used for IO is not currently open
    FileHandleInvalid,
    /// The file handle used for IO is not the correct type for that operation
    FileHandleWrongType,
    /// An IO operation failed
    OperationFailed,
    /// Attempted a FS method that isn't supported by the driver
    UnsupportedOperation,
    /// Sent a control command that was not recognized
    UnsupportedCommand,
    /// Attempted to open or bind a handle that is already open
    AlreadyOpen,
    /// Tried to write to a closed pipe / socket / etc
    WriteToClosedIO,
    /// Sent an invalid struct or argument to an IO Op
    InvalidArgument,

    Unknown = 0xffffffff,
}

impl TryFrom<u32> for IOError {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::FileSystemError),
            2 => Ok(Self::NotFound),
            3 => Ok(Self::FileHandleInvalid),
            4 => Ok(Self::FileHandleWrongType),
            5 => Ok(Self::OperationFailed),
            6 => Ok(Self::UnsupportedOperation),
            7 => Ok(Self::UnsupportedCommand),
            8 => Ok(Self::AlreadyOpen),
            9 => Ok(Self::WriteToClosedIO),
            10 => Ok(Self::InvalidArgument),
            _ => Ok(Self::Unknown),
        }
    }
}

impl Into<u32> for IOError {
    fn into(self) -> u32 {
        self as u32
    }
}
