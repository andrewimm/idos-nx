pub struct Environment {
    cwd: [u8; 256],
    cwd_length: usize,
}

impl Environment {
    pub fn new(drive: &str) -> Self {
        let mut cwd = [0; 256];
        let drive_bytes = drive.as_bytes();
        cwd[..drive_bytes.len()].copy_from_slice(drive_bytes);
        cwd[drive_bytes.len()] = b'\\';
        Self {
            cwd,
            cwd_length: drive_bytes.len() + 1,
        }
    }

    pub fn put_cwd(&mut self, buffer: &mut [u8]) -> usize {
        let mut i = 0;
        while i < self.cwd_length && i < buffer.len() {
            buffer[i] = self.cwd[i];
            i += 1;
        }
        self.cwd_length
    }

    pub fn cwd_bytes(&self) -> &[u8] {
        &self.cwd[..self.cwd_length]
    }

    pub fn pushd(&mut self, dir_bytes: &[u8]) {
        if self.cwd_length + dir_bytes.len() + 1 < self.cwd.len() {
            self.cwd[self.cwd_length..self.cwd_length + dir_bytes.len()].copy_from_slice(dir_bytes);
            self.cwd_length += dir_bytes.len();
            self.cwd[self.cwd_length] = b'\\';
            self.cwd_length += 1;
        }
    }
}
