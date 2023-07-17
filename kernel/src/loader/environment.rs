use alloc::vec::Vec;
use crate::task::memory::ExecutionSegment;

pub struct ExecutionEnvironment {
    pub registers: InitialRegisters,
    pub segments: Vec<ExecutionSegment>,
    pub require_vm: bool,
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

