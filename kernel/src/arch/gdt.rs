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

    pub fn set_base(&mut self, base: u32) {
    }

    pub fn set_limit(&mut self, limit: u32) {
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

#[repr(C, packed)]
pub struct TaskStateSegment {
    pub prev_tss: u32,
    pub esp0: u32,
    pub ss0: u32,
    pub esp1: u32,
    pub ss1: u32,
    pub esp2: u32,
    pub ss2: u32,
    pub cr3: u32,
    pub eip: u32,
    pub eflags: u32,
    pub eax: u32,
    pub ecx: u32,
    pub edx: u32,
    pub ebx: u32,
    pub esp: u32,
    pub ebp: u32,
    pub esi: u32,
    pub edi: u32,
    pub es: u32,
    pub cs: u32,
    pub ss: u32,
    pub ds: u32,
    pub fs: u32,
    pub gs: u32,
    pub ldt: u32,
    pub trap: u16,
    pub iomap_base: u16,
}

impl TaskStateSegment {
    pub fn zero(&mut self) {
      self.prev_tss = 0;
      self.esp0 = 0;
      self.ss0 = 0;
      self.esp1 = 0;
      self.ss1 = 0;
      self.esp2 = 0;
      self.ss2 = 0;
      self.cr3 = 0;
      self.eip = 0;
      self.eflags = 0;
      self.eax = 0;
      self.ecx = 0;
      self.edx = 0;
      self.ebx = 0;
      self.esp = 0;
      self.ebp = 0;
      self.esi = 0;
      self.edi = 0;
      self.es = 0;
      self.cs = 0;
      self.ss = 0;
      self.ds = 0;
      self.fs = 0;
      self.gs = 0;
      self.ldt = 0;
      self.trap = 0;
      self.iomap_base = 0;
    }
}

#[repr(C, packed)]
pub struct TssWithBitmap {
    pub tss: TaskStateSegment,
    pub bitmap: [u8; 128],
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

pub static mut TSS: TssWithBitmap = TssWithBitmap {
    tss: TaskStateSegment {
        prev_tss: 0,
        esp0: 0,
        ss0: 0,
        esp1: 0,
        ss1: 0,
        esp2: 0,
        ss2: 0,
        cr3: 0,
        eip: 0,
        eflags: 0,
        eax: 0,
        ecx: 0,
        edx: 0,
        ebx: 0,
        esp: 0,
        ebp: 0,
        esi: 0,
        edi: 0,
        es: 0,
        cs: 0,
        ss: 0,
        ds: 0,
        fs: 0,
        gs: 0,
        ldt: 0,
        trap: 0,
        iomap_base: 0,
    },
    bitmap: [0; 128],
};

pub fn set_tss_stack_pointer(sp: u32) {
    unsafe {
        TSS.tss.esp0 = sp;
    }
}

pub fn init_tss() {
    unsafe {
        TSS.tss.ss0 = 0x10;
        TSS.bitmap[127] = 0xff;
        GDT[5].set_base(&TSS as *const TssWithBitmap as u32);
        GDT[5].set_limit(core::mem::size_of::<TssWithBitmap>() as u32 - 1);
    }
}

pub fn ltr(index: u16) {
    let segment = index | 3;
    unsafe {
        asm!(
            "ltr {s:x}",
            s = in(reg) segment,
        );
    }
}

