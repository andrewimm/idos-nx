/// Used to set register state when a task begins
#[repr(C, packed)]
pub struct EnvironmentRegisters {
    // registers that get popped by entry code
    pub eax: u32,
    pub ecx: u32,
    pub edx: u32,
    pub ebx: u32,
    pub ebp: u32,
    pub esi: u32,
    pub edi: u32,

    // registers that get popped by the iretd command
    pub eip: u32,
    pub cs: u32,
    pub flags: u32,
    pub esp: u32,
    pub ss: u32,

    pub es: u32,
    pub ds: u32,
    pub fs: u32,
    pub gs: u32,
}

