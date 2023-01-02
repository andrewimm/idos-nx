use core::arch::asm;
use crate::arch::segment::SegmentSelector;
use super::stack::StackFrame;

// Flags used in IDT entries
pub const IDT_PRESENT: u8 = 1 << 7;
pub const IDT_DESCRIPTOR_RING_0: u8 = 0;
pub const IDT_DESCRIPTOR_RING_3: u8 = 3 << 5;
pub const IDT_GATE_TYPE_INT_32: u8 = 0xe;

pub type HandlerFunction = unsafe extern "x86-interrupt" fn(StackFrame);

/// An IDT Entry tells the x86 CPU how to handle an interrupt.
/// The entry attributes determine how the interrupt is entered, what permission
/// ring and memory selector to use, and which address to enter.
#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct IdtEntry {
    pub offset_low: u16,
    pub selector: SegmentSelector,
    pub zero: u8,
    pub type_and_attributes: u8,
    pub offset_high: u16,
}

impl IdtEntry {
    pub const fn new() -> Self {
        Self {
            offset_low: 0,
            selector: SegmentSelector::new(1, 0),
            zero: 0,
            type_and_attributes: 0,
            offset_high: 0,
        }
    }

    /// Set the handler function for this entry. When this interrupt occurs,
    /// the CPU will attempt to enter this function.
    pub fn set_handler_(&mut self, func: HandlerFunction) {
        let offset = func as *const () as usize;
        self.set_handler_at_offset(offset);
    }

    /// The actual implementation for setting the handler data
    fn set_handler_at_offset(&mut self, offset: usize) {
        self.offset_low = offset as u16;
        self.offset_high = (offset >> 16) as u16;
        self.type_and_attributes = IDT_PRESENT | IDT_GATE_TYPE_INT_32;
    }

    /// Allow the interrupt to be called from Ring 3. This is required for any
    /// syscalls.
    pub fn make_usermode_accessible(&mut self) {
        self.type_and_attributes |= IDT_DESCRIPTOR_RING_3;
    }
}

/// The IDT Descriptor is a special in-memory data structure that tells the CPU
/// how to find the actual IDT table. Because the CPU needs to know how many
/// valid entries exist in the table, it requires this extra layer of
/// indirection.
#[repr(C, packed)]
pub struct IdtDescriptor {
    pub size: u16,
    pub offset: u32,
}

impl IdtDescriptor {
    pub const fn new() -> Self {
        Self {
            size: 0,
            offset: 0,
        }
    }

    pub fn point_to(&mut self, idt: &[IdtEntry]) {
        self.size = (idt.len() * core::mem::size_of::<IdtEntry>() - 1) as u16;
        self.offset = &idt[0] as *const IdtEntry as u32;
    }

    pub fn load(&self) {
        unsafe {
            asm!(
                "lidt [{desc}]",
                desc = in(reg) self,
                options(preserves_flags, nostack),
            );
        }
    }
}

// Global tables and structures:

pub static mut IDTR: IdtDescriptor = IdtDescriptor::new();

pub static mut IDT: [IdtEntry; 256] = [IdtEntry::new(); 256];

pub unsafe fn init_idt() {
    IDTR.point_to(&IDT);

    // Set exception handlers. Because all interrupt handlers are currently
    // hard-coded to be Interrupt Gate types (vs Task), they will disable other
    // interrupts when triggered. If we make the kernel interrupt-safe, these
    // can be updated to tasks and made interruptable themselves.
   
    // TODO: set handlers for 0x00..0x0f

    // Interrupts through 0x1f represent exceptions that we don't handle,
    // usually because they are deprecated or represent unsupported hardware.
    
    // Interrupts 0x20-0x2f are mostly unused, to avoid conflict with legacy
    // DOS interrupts. The only one used by the kernel is 0x2b, which is the
    // entrypoint for user-mode programs to make a syscall.

    // TODO: set usermode-accessible interrupt for the syscall handler at 0x2b

    // Interrupts 0x30-0x3f are reserved for PIC hardware interrupts.
    // This is where we begin to allow processes to install their own interrupt
    // handlers. For example, a COM driver would want to listen to interrupt
    // 0x34. To accommodate this, these interrupts have a handler that runs
    // through a vector of installed hooks before returning to whatever code
    // was originally running before the interrupt.

    // TODO: Set up hooks for installed handlers

    IDTR.load();
}
