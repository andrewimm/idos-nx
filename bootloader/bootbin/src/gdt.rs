use core::arch::asm;

#[repr(C, packed)]
pub struct GDT {
    null_entry: u64,
    code_entry: u64,
    data_entry: u64,
}

impl GDT {
    pub const fn new() -> GDT {
        let limit = 0xfffff;

        let limit_low = limit & 0xffff;
        let limit_high = (limit & 0xf0000) << 32;
        let base = 0;
        let access = (
            0x80 | // present
            0x10 | // code/data descriptor
            0x02   // read/write
        ) << 40;
        let flags = 0b1100 << 52;

        let entry = limit_low | base | access | limit_high | flags; 

        GDT {
            null_entry: 0,
            code_entry: entry | (0x08 << 40),
            data_entry: entry,
        }
    }
}

pub static INITIAL_GDT: GDT = GDT::new();

#[repr(C, packed)]
pub struct GdtPointer {
    pub limit: u16,
    pub base: u32,
}

impl GdtPointer {
    pub fn new() -> GdtPointer {
        let limit = (core::mem::size_of::<GDT>() - 1) as u16;
        let base = &INITIAL_GDT as *const GDT as u32;

        GdtPointer {
            limit,
            base,
        }
    }

    pub fn load(&self) {
        unsafe {
            asm!(
                "lgdt [{}]",
                in(reg) self,
                options(nostack, preserves_flags),
            );
        }
    }
}

