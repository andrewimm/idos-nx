pub enum SeekMethod {
    Absolute(usize),
    Relative(isize),
}

impl SeekMethod {
    pub fn from_current_position(&self, current: usize) -> usize {
        match self {
            SeekMethod::Absolute(pos) => *pos,
            SeekMethod::Relative(off) => (current as isize).saturating_add(*off) as usize,
        }
    }

    pub fn encode(&self) -> (u32, u32) {
        match self {
            SeekMethod::Absolute(pos) => (1, *pos as u32),
            SeekMethod::Relative(off) => (2, *off as u32),
        }
    }

    pub fn decode(method: u32, delta: u32) -> Option<Self> {
        match method {
            1 => Some(SeekMethod::Absolute(delta as usize)),
            2 => Some(SeekMethod::Relative(delta as isize)),
            _ => None,
        }
    }
}
