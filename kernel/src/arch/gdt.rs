use core::arch::asm;

const GDT_ACCESS_PRESENT: u8 = 1 << 7;
const GDT_ACCESS_RING_0: u8 = 0;
const GDT_ACCESS_RING_3: u8 = 3 << 5;
const GDT_ACCESS_CODE_DATA_DESCRIPTOR: u8 = 1 << 4;
const GDT_ACCESS_SYSTEM_DESCRIPTOR: u8 = 0;
const GDT_ACCESS_EXECUTABLE: u8 = 1 << 3;
const GDT_ACCESS_RW: u8 = 1 << 1;

const GDT_FLAG_GRANULARITY_4KB: u8 = 1 << 7;
const GDT_FLAG_SIZE_32_BIT: u8 = 1 << 6;

#[repr(C, packed)]
pub struct GdtEntry {
    pub limit_low: u16,
    pub base_low: u16,
    pub base_middle: u8,
    pub access: u8,
    pub flags_and_limit_high: u8,
    pub base_high: u8,
}

impl GdtEntry {
    pub const fn new(base: u32, limit: u32, access: u8, flags: u8) -> Self {
        let limit_low = (limit & 0xffff) as u16;
        let limit_high = ((limit >> 16) & 0x000f) as u8;
        let base_low = (base & 0xffff) as u16;
        let base_middle = ((base >> 16) & 0xff) as u8;
        let base_high = ((base >> 24) & 0xff) as u8;
        
        Self {
            limit_low,
            base_low,
            base_middle,
            access,
            flags_and_limit_high: (flags & 0xf0) | limit_high,
            base_high,
        }
    }
}

#[repr(C, packed)]
pub struct GdtDescriptor {
    pub size: u16,
    pub offset: u32,
}

impl GdtDescriptor {
    pub const fn new() -> Self {
        Self {
            size: 0,
            offset: 0,
        }
    }

    pub fn point_to(&mut self, gdt: &[GdtEntry]) {
        self.size = (gdt.len() * core::mem::size_of::<GdtEntry>() - 1) as u16;
        self.offset = &gdt[0] as *const GdtEntry as u32;
    }

    pub fn load(&self) {
        unsafe {
            asm!(
                "lgdt [{desc}]",
                desc = in(reg) self,
                options(preserves_flags, nostack),
            );
        }
    }
}

// Global Tables and Structures:

pub static mut GDTR: GdtDescriptor = GdtDescriptor::new();

pub static mut GDT: [GdtEntry; 6] = [
    // 0x00: Null entry
    GdtEntry::new(0, 0, 0, 0),

    // 0x08: Kernel code
    GdtEntry::new(
        0,
        0xffffffff,
        GDT_ACCESS_PRESENT | GDT_ACCESS_RING_0 | GDT_ACCESS_CODE_DATA_DESCRIPTOR | GDT_ACCESS_EXECUTABLE | GDT_ACCESS_RW,
        GDT_FLAG_GRANULARITY_4KB | GDT_FLAG_SIZE_32_BIT,
    ),

    // 0x10: Kernel data
    GdtEntry::new(
        0,
        0xffffffff,
        GDT_ACCESS_PRESENT | GDT_ACCESS_RING_0 | GDT_ACCESS_CODE_DATA_DESCRIPTOR | GDT_ACCESS_RW,
        GDT_FLAG_GRANULARITY_4KB | GDT_FLAG_SIZE_32_BIT,
    ),

    // 0x18: Userspace code
    GdtEntry::new(
        0,
        0xffffffff,
        GDT_ACCESS_PRESENT | GDT_ACCESS_RING_3 | GDT_ACCESS_CODE_DATA_DESCRIPTOR | GDT_ACCESS_EXECUTABLE | GDT_ACCESS_RW,
        GDT_FLAG_GRANULARITY_4KB | GDT_FLAG_SIZE_32_BIT,
    ),

    // 0x20: Userspace data
    GdtEntry::new(
        0,
        0xffffffff,
        GDT_ACCESS_PRESENT | GDT_ACCESS_RING_3 | GDT_ACCESS_CODE_DATA_DESCRIPTOR | GDT_ACCESS_RW,
        GDT_FLAG_GRANULARITY_4KB | GDT_FLAG_SIZE_32_BIT,
    ),

    // 0x28: TSS
    GdtEntry::new(
        0,
        0xffffffff,
        GDT_ACCESS_PRESENT | GDT_ACCESS_RING_3 | GDT_ACCESS_SYSTEM_DESCRIPTOR | 0x09, // 0x09 = 32-bit TSS Available
        0,
    ),
];

