static mut ARGC: u32 = 0;
static mut ARGV: *const u32 = core::ptr::null();

pub fn init_args(argc: u32, argv: *const u32) {
    unsafe {
        ARGC = argc;
        ARGV = argv;
    }
}

pub fn args() -> Args {
    Args::new()
}

pub struct Args {
    count: u32,
    current_index: u32,
    base_ptr: *const u32,
}

impl Args {
    pub fn new() -> Self {
        Self {
            count: unsafe { ARGC },
            current_index: 0,
            base_ptr: unsafe { ARGV },
        }
    }

    pub unsafe fn get_current(&self) -> &'static str {
        let ptr = self.base_ptr.offset(self.current_index as isize);
        let base = *ptr as *const u8;
        let mut addr = base;
        let mut len = 0;
        while *addr != 0 {
            len += 1;
            addr = addr.offset(1);
        }
        
        let str_bytes = core::slice::from_raw_parts(base, len);
        core::str::from_utf8_unchecked(str_bytes)
    }
}

impl Iterator for Args {
    type Item = &'static str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index >= self.count {
            return None;
        }

        let item = unsafe { self.get_current() };
        self.current_index += 1;
        Some(item)
    }
}
