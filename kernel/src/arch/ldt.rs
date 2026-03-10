use core::arch::asm;

use super::gdt::GdtEntry;

/// Maximum number of LDT entries per task.
/// DPMI programs typically use only a handful of descriptors.
pub const LDT_MAX_ENTRIES: usize = 64;

/// A single Local Descriptor Table entry — same 8-byte packed format as a GDT entry.
pub type LdtEntry = GdtEntry;

/// Per-task Local Descriptor Table.
/// Contains up to LDT_MAX_ENTRIES segment descriptors that can be used
/// by DPMI protected-mode programs.
pub struct LocalDescriptorTable {
    pub entries: [LdtEntry; LDT_MAX_ENTRIES],
}

impl LocalDescriptorTable {
    pub fn new() -> Self {
        Self {
            entries: core::array::from_fn(|_| LdtEntry::new(0, 0, 0, 0)),
        }
    }

    /// Allocate the first free LDT entry. Returns the index, or None if full.
    /// Slot 0 (null descriptor) is reserved and never allocated.
    pub fn allocate(&mut self) -> Option<usize> {
        for i in 1..LDT_MAX_ENTRIES {
            if !self.entries[i].is_present() {
                return Some(i);
            }
        }
        None
    }

    /// Free an LDT entry by index. Returns false if index is invalid or already free.
    pub fn free(&mut self, index: usize) -> bool {
        if index == 0 || index >= LDT_MAX_ENTRIES {
            return false;
        }
        self.entries[index] = LdtEntry::new(0, 0, 0, 0);
        true
    }

    /// Get the byte size of the table (for the GDT LDT descriptor limit).
    pub fn byte_size(&self) -> u32 {
        (LDT_MAX_ENTRIES * core::mem::size_of::<LdtEntry>()) as u32
    }

    /// Get the linear address of the table.
    pub fn base_address(&self) -> u32 {
        &self.entries[0] as *const LdtEntry as u32
    }
}

/// GDT index reserved for the LDT descriptor.
pub const GDT_LDT_INDEX: usize = 8;

/// Segment selector for the LDT descriptor in the GDT (index 8, GDT, RPL 0).
pub const GDT_LDT_SELECTOR: u16 = (GDT_LDT_INDEX as u16) << 3;

/// Load the LDT register. If `selector` is 0, the LDT is disabled.
pub fn lldt(selector: u16) {
    unsafe {
        asm!(
            "lldt {sel:x}",
            sel = in(reg) selector,
            options(preserves_flags, nostack),
        );
    }
}

/// Update the GDT's LDT descriptor to point at the given LDT, then load it.
/// Call with `None` to disable the LDT for the current task.
pub fn load_task_ldt(gdt: &mut [GdtEntry], ldt: Option<&LocalDescriptorTable>) {
    match ldt {
        Some(ldt) => {
            let base = ldt.base_address();
            let limit = ldt.byte_size() - 1;
            // System descriptor type 0x02 = LDT, present, DPL 0
            let access = 0x82; // Present | LDT type
            gdt[GDT_LDT_INDEX] = GdtEntry::new(base, limit, access, 0);
            lldt(GDT_LDT_SELECTOR);
        }
        None => {
            gdt[GDT_LDT_INDEX] = GdtEntry::new(0, 0, 0, 0);
            lldt(0);
        }
    }
}
