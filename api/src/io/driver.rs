/// DriverCommand is an enum shared between the kernel and user-space drivers,
/// used to encode / decode messages sent to Async IO drivers.
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
