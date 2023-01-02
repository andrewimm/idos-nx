/// A segment selector encodes a GDT entry and a privilege level into a single
/// u16 value
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct SegmentSelector(u16);

impl SegmentSelector {
    pub const fn new(table_index: u16, privilege_level: u16) -> Self {
        Self(
            (table_index << 3) |
            (privilege_level & 3)
        )
    }
}
