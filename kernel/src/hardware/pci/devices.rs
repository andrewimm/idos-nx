#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum DeviceClass {
    Unclassified = 0,
    MassStorage = 1,
    Network = 2,
    Display = 3,
    Multimedia = 4,
    Memory = 5,
    Bridge = 6,
    Communication = 7,
    BaseSystem = 8,
    Input = 9,
    Docking = 0xa,
    Processor = 0xb,
    SerialBus = 0xc,
    Wireless = 0xd,
}

