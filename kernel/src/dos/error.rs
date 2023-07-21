#[repr(u16)]
pub enum DosErrorCode {
    InvalidFunction = 1,
    FileNotFound,
    PathNotFound,
    TooManyOpenFiles,
    AccessDenied,
    InvalidHandle,
    McbDestroyed,
    InsufficientMemory,
    InvalidMemoryBlockAddress,
    InvalidEnvironment,
    InvalidFormat,
    InvalidAccessCode,
    InvalidData,

    InvalidDrive = 0x0f,
    CannotRemoveDir,
    NotSameDevice,
    NoMatchingFiles,
}
