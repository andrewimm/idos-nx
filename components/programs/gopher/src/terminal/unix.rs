use std::mem::MaybeUninit;

use libc::{termios, tcgetattr, STDIN_FILENO, tcsetattr, read, write, STDOUT_FILENO, size_t, c_void, ECHO, TCSAFLUSH, ICANON};
use super::TerminalControl;

pub struct PlatformTerminalControl {
    orig_state: Option<termios>,
}

impl TerminalControl for PlatformTerminalControl {
    fn new() -> Self {
        Self {
            orig_state: None,
        }
    }
    
    fn set_raw_mode(&mut self) {
        let mut term_state = MaybeUninit::<termios>::uninit();
        let mut ready_state = unsafe {
            let _ = tcgetattr(STDIN_FILENO, term_state.as_mut_ptr());
            term_state.assume_init()
        };
        self.orig_state = Some(ready_state);

        ready_state.c_lflag &= !(ECHO | ICANON);
        unsafe {
            let _ = tcsetattr(STDIN_FILENO, TCSAFLUSH, &ready_state as *const termios);
        }
    }

    fn restore(&mut self) {
        if let Some(term_state) = self.orig_state.take() {
            unsafe {
                let _ = tcsetattr(STDIN_FILENO, TCSAFLUSH, &term_state as *const termios);
            }
        }
    }

    fn read(&self, buffer: &mut [u8]) -> usize {
        unsafe {
            let res = read(STDIN_FILENO, &mut buffer[0] as *mut _ as *mut c_void, buffer.len() as size_t);
            res as usize
        }
        
    }

    fn write(&self, buffer: &[u8]) -> usize {
        unsafe {
            let res = write(STDOUT_FILENO, &buffer[0] as *const _ as *const c_void, buffer.len() as size_t);
            res as usize
        }
    }
}

impl Drop for PlatformTerminalControl {
    fn drop(&mut self) {
        self.restore();
    }
}
