#[repr(C, packed)]
pub struct EthernetFrame {
    pub dest_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ethertype: u16,
}

impl EthernetFrame {
    pub fn new(src: [u8; 6], dest: [u8; 6], ethertype: u16) -> Self {
        Self {
            src_mac: src,
            dest_mac: dest,
            ethertype: ethertype.to_be(),
        }
    }

    pub fn broadcast_arp(src: [u8; 6]) -> Self {
        Self::new(src, [0xff; 6], 0x0806)
    }

    pub fn as_buffer(&self) -> &[u8] {
        let ptr = self as *const Self as *const u8;
        let size = core::mem::size_of::<Self>();
        unsafe {
            core::slice::from_raw_parts(ptr, size)
        }
    }
}

