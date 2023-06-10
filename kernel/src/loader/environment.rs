use alloc::vec::Vec;
use crate::task::memory::ExecutionSegment;

pub struct ExecutionEnvironment {
    pub registers: InitialRegisters,
    pub segments: Vec<ExecutionSegment>,
}

pub struct InitialRegisters {
    pub eax: Option<u32>,

    pub eip: Option<u32>,
    pub esp: Option<u32>,

    pub cs: Option<u32>,
    pub ds: Option<u32>,
    pub es: Option<u32>,
    pub ss: Option<u32>,
}

