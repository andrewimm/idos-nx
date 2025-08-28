//! Consoles need to support a lot of IOCTL commands to modify functionality.
//! All of that is handled here.

// consts for supported IOCTL commands

pub const TCGETS: u32 = 0x5401;
pub const TCSETS: u32 = 0x5402;
pub const TCSETSW: u32 = 0x5403;
pub const TCSETSF: u32 = 0x5404;
pub const TIOCGWINSZ: u32 = 0x5413;
pub const TIOCSWINSZ: u32 = 0x5414;

// custom IOCTLs for graphics mode
/// Enable raw graphics mode
/// To enter graphics mode, the user must provide a GraphicsMode struct
/// with the desired width, height, and bpp. The framebuffer field is ignored.
/// On success, the framebuffer field will be filled with the physical address
/// of the framebuffer.
pub const TSETGFX: u32 = 0x6001;
/// Disable raw graphics mode, return to text mode
pub const TSETTEXT: u32 = 0x6002;

// TERMIOS: structure for getting / setting attributes
#[repr(C, packed)]
#[derive(Clone)]
pub struct Termios {
    pub iflags: u32,
    pub oflags: u32,
    pub cflags: u32,
    pub lflags: u32,
    pub cc: [u8; 20],
}

impl Termios {
    pub const fn default() -> Self {
        Self {
            iflags: 0,
            oflags: 0,
            cflags: 0,
            lflags: 0,
            cc: [0; 20],
        }
    }
}

// TERMIOS: local flags
pub const ISIG: u32 = 0x00000001;
pub const ICANON: u32 = 0x00000002;
pub const ECHO: u32 = 0x00000008;
pub const ECHOE: u32 = 0x00000010;
pub const ECHOK: u32 = 0x00000020;
pub const ECHONL: u32 = 0x00000040;
pub const NOFLSH: u32 = 0x00000080;
pub const TOSTOP: u32 = 0x00000100;

// WINDOW SIZE: structure for getting / setting window size
#[repr(C, packed)]
pub struct WinSize {
    pub rows: u16,
    pub cols: u16,
    pub xpixel: u16,
    pub ypixel: u16,
}

// GRAPHICS MODE: structure for getting / setting graphics mode
#[repr(C, packed)]
pub struct GraphicsMode {
    pub width: u32,
    pub height: u32,
    pub bpp: u32,
    pub framebuffer: u32, // physical address of the framebuffer
}
