use idos_api::io::handle::Handle;

use alloc::string::String;

pub struct Environment {
    cwd: [u8; 256],
    cwd_length: usize,
    pub stdin: Handle,
    pub stdout: Handle,
    prompt_fmt: [u8; 128],
    prompt_fmt_len: usize,
}

impl Environment {
    pub fn new(drive: &str) -> Self {
        let mut cwd = [0; 256];
        let drive_bytes = drive.as_bytes();
        cwd[..drive_bytes.len()].copy_from_slice(drive_bytes);
        cwd[drive_bytes.len()] = b'\\';
        let mut prompt_fmt = [0u8; 128];
        prompt_fmt[..4].copy_from_slice(b"$P$G");
        Self {
            cwd,
            cwd_length: drive_bytes.len() + 1,
            stdin: Handle::new(0),
            stdout: Handle::new(1),
            prompt_fmt,
            prompt_fmt_len: 4,
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

    pub fn cwd_string(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.cwd[..self.cwd_length]) }
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
        self.cwd[..drive.len()].copy_from_slice(drive);
        self.cwd[drive.len()] = b'\\';
        self.cwd_length = drive.len() + 1;
    }

    pub fn set_prompt(&mut self, fmt: &[u8]) {
        let len = fmt.len().min(self.prompt_fmt.len());
        self.prompt_fmt[..len].copy_from_slice(&fmt[..len]);
        self.prompt_fmt_len = len;
    }

    /// Expand the prompt format string into `out`, returning the number of
    /// bytes written. Uses DOS PROMPT substitution codes:
    ///   $P = current directory, $G = >, $L = <, $N = current drive letter,
    ///   $$ = literal $, $E = ESC, $_ = CR+LF, $H = backspace,
    ///   $B = |, $Q = =, $S = space
    pub fn expand_prompt(&self, out: &mut [u8]) -> usize {
        let fmt = &self.prompt_fmt[..self.prompt_fmt_len];
        let mut i = 0;
        let mut w = 0;
        while i < fmt.len() && w < out.len() {
            if fmt[i] == b'$' && i + 1 < fmt.len() {
                i += 1;
                let ch = fmt[i];
                // For letters, fold to lowercase. Leave non-letters as-is.
                let key = if ch.is_ascii_alphabetic() { ch | 0x20 } else { ch };
                match key {
                    b'p' => {
                        let cwd = self.cwd_bytes();
                        let n = cwd.len().min(out.len() - w);
                        out[w..w + n].copy_from_slice(&cwd[..n]);
                        w += n;
                    }
                    b'n' => {
                        if w < out.len() && self.cwd_length > 0 {
                            out[w] = self.cwd[0];
                            w += 1;
                        }
                    }
                    b'g' => { out[w] = b'>'; w += 1; }
                    b'l' => { out[w] = b'<'; w += 1; }
                    b'b' => { out[w] = b'|'; w += 1; }
                    b'q' => { out[w] = b'='; w += 1; }
                    b's' => { out[w] = b' '; w += 1; }
                    b'h' => { out[w] = 0x08; w += 1; }
                    b'e' => { out[w] = 0x1b; w += 1; }
                    b'$' => { out[w] = b'$'; w += 1; }
                    b'_' => {
                        if w + 1 < out.len() {
                            out[w] = b'\r';
                            out[w + 1] = b'\n';
                            w += 2;
                        }
                    }
                    _ => {}
                }
                i += 1;
            } else {
                out[w] = fmt[i];
                w += 1;
                i += 1;
            }
        }
        w
    }

    pub fn full_file_path(&self, file: &String) -> String {
        // TODO: check if `file` is absolute, if so, return it as is

        let mut full_path = String::from(self.cwd_string());
        let mut split_iter = file.split('\\').peekable();
        loop {
            match split_iter.next() {
                Some(chunk) => match chunk {
                    "." => continue,
                    ".." => {
                        if full_path.len() < 2 {
                            continue;
                        }
                        let _ = full_path.pop();
                        while !full_path.ends_with('\\') && !full_path.ends_with(':') {
                            let _ = full_path.pop();
                        }
                        if full_path.ends_with(':') {
                            full_path.push('\\');
                        }
                    }
                    dir => {
                        if !dir.is_empty() {
                            full_path.push_str(dir);
                            if split_iter.peek().is_some() {
                                full_path.push('\\');
                            }
                        }
                    }
                },
                None => break,
            }
        }
        full_path
    }
}
