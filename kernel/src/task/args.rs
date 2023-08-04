use alloc::vec::Vec;

pub struct ExecArgs {
    raw: Vec<u8>,
    lengths: Vec<u32>,
}

impl ExecArgs {
    pub fn new() -> Self {
        Self {
            raw: Vec::new(),
            lengths: Vec::new(),
        }
    }

    pub fn add(&mut self, arg: &str) -> &mut Self {
        self.lengths.push(arg.len() as u32 + 1);
        for b in arg.bytes() {
            self.raw.push(b);
        }
        self.raw.push(0);
        self
    }

    pub fn arg_string(&self) -> &Vec<u8> {
        &self.raw
    }

    pub fn arg_lengths(&self) -> &Vec<u32> {
        &self.lengths
    }

    pub fn arg_count(&self) -> u32 {
        self.lengths.len() as u32
    }

    pub fn stack_size(&self) -> u32 {
        let mut string_length = self.raw.len();
        if string_length & 3 != 0 {
            string_length += 4 - (string_length & 3);
        }
        (string_length + self.lengths.len() * 4 + 4) as u32
    }
}
