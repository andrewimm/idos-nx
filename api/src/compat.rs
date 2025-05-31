#[derive(Clone)]
pub struct VMRegisters {
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
    pub esi: u32,
    pub edi: u32,
    pub ebp: u32,

    pub eip: u32,
    pub cs: u32,
    pub eflags: u32,
    pub esp: u32,
    pub ss: u32,

    pub es: u32,
    pub ds: u32,
    pub fs: u32,
    pub gs: u32,
}

impl VMRegisters {
    pub fn ah(&self) -> u8 {
        ((self.eax & 0xff00) >> 8) as u8
    }

    pub fn set_al(&mut self, al: u8) {
        self.eax &= 0xffffff00;
        self.eax |= al as u32;
    }

    pub fn dl(&self) -> u8 {
        ((self.edx & 0xff00) >> 8) as u8
    }
}
