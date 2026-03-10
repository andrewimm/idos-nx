use crate::compat::LdtDescriptorParams;

/// Allocate an LDT descriptor. Returns the selector (with TI=1, RPL=3),
/// or 0xffff_ffff if the LDT is full.
pub fn ldt_allocate() -> u32 {
    super::syscall(0x08, 0, 0, 0)
}

/// Modify an LDT descriptor's base, limit, access, and flags in one call.
/// Returns 0 on success.
pub fn ldt_modify(selector: u32, params: &LdtDescriptorParams) -> u32 {
    super::syscall(0x09, selector, params as *const LdtDescriptorParams as u32, 0)
}

/// Free an LDT descriptor. Returns 0 on success.
pub fn ldt_free(selector: u32) -> u32 {
    super::syscall(0x0a, selector, 0, 0)
}
