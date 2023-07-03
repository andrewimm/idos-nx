use alloc::vec::Vec;
use super::ip::IPV4Address;

#[repr(C, packed)]
pub struct DhcpPacket {
    pub op: u8,
    pub htype: u8,
    pub hlen: u8,
    pub hops: u8,

    pub xid: u32,

    pub secs: u16,
    pub flags: u16,

    pub ciaddr: IPV4Address,
    pub yiaddr: IPV4Address,
    pub siaddr: IPV4Address,
    pub giaddr: IPV4Address,

    pub chaddr: [u8; 16],
    pub sname: [u8; 64],
    pub file: [u8; 128],
}

impl DhcpPacket {
    pub fn as_buffer(&self) -> &[u8] {
        let ptr = self as *const Self as *const u8;
        let size = core::mem::size_of::<Self>();
        unsafe {
            core::slice::from_raw_parts(ptr, size)
        }
    }
}

pub fn discover_packet(mac: [u8; 6], xid: u32) -> Vec<u8> {
    let mut packet = DhcpPacket {
        op: 1,
        htype: 1,
        hlen: 6,
        hops: 0,

        xid,

        secs: 0,
        flags: 0,

        ciaddr: IPV4Address([0, 0, 0, 0]),
        yiaddr: IPV4Address([0, 0, 0, 0]),
        siaddr: IPV4Address([0, 0, 0, 0]),
        giaddr: IPV4Address([0, 0, 0, 0]),

        chaddr: [0; 16],
        sname: [0; 64],
        file: [0; 128],
    };

    for i in 0..6 {
        packet.chaddr[i] = mac[i];
    }

    let options: &[u8] = &[
        0x63, 0x82, 0x53, 0x63,
        0x35, 0x01, 0x01,

        0xff,
    ];

    let packet_size = core::mem::size_of::<DhcpPacket>();
    let total_len = packet_size + options.len();

    let mut packet_data = Vec::with_capacity(total_len);
    for i in 0..total_len {
        packet_data.push(0);
    }

    packet_data.as_mut_slice()[..packet_size].copy_from_slice(packet.as_buffer());
    packet_data.as_mut_slice()[packet_size..].copy_from_slice(options);

    packet_data
}
