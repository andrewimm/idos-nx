use idos_api::io::handle::Handle;

pub struct Environment {
    cwd: [u8; 256],
    cwd_length: usize,
    pub stdin: Handle,
    pub stdout: Handle,
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
            stdin: Handle::new(0),
            stdout: Handle::new(1),
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

    pub fn popd(&mut self) {
        if self.cwd_length < 2 {
            return;
        }
        let mut i = self.cwd_length - 2;
        while i > 0 && self.cwd[i] != b'\\' {
            i -= 1;
        }
        if self.cwd[i] == b'\\' {
            self.cwd_length = i + 1;
        }
    }

    pub fn pop_to_root(&mut self) {
        for i in 0..self.cwd_length {
            if self.cwd[i] == b'\\' {
                self.cwd_length = i + 1;
                return;
            }
        }
    }

    pub fn reset_drive(&mut self, drive: &[u8]) {
        self.cwd.copy_from_slice(drive);
        self.cwd[drive.len()] = b':';
        self.cwd_length = drive.len() + 1;
    }
}
