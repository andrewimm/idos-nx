use core::sync::atomic::{AtomicU32, Ordering};

use alloc::{vec::Vec, collections::BTreeSet};
use spin::RwLock;
use crate::task::id::TaskID;

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

        xid: xid.to_be(),

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

pub fn request_packet(mac: [u8; 6], server_ip: IPV4Address, requested_ip: IPV4Address, xid: u32) -> Vec<u8> {
    let mut packet = DhcpPacket {
        op: 1,
        htype: 1,
        hlen: 6,
        hops: 0,

        xid: xid.to_be(),

        secs: 0,
        flags: 0,

        ciaddr: IPV4Address([0, 0, 0, 0]),
        yiaddr: IPV4Address([0, 0, 0, 0]),
        siaddr: server_ip,
        giaddr: IPV4Address([0, 0, 0, 0]),

        chaddr: [0; 16],
        sname: [0; 64],
        file: [0; 128],
    };

    for i in 0..6 {
        packet.chaddr[i] = mac[i];
    }

    let options: &[u8] = &[
        // magic cookie
        0x63, 0x82, 0x53, 0x63,
        // DHCP request
        0x35, 0x01, 0x03,
        // requested IP
        0x32, 0x04, requested_ip[0], requested_ip[1], requested_ip[2], requested_ip[3],
        // server IP
        0x36, 0x04, server_ip[0], server_ip[1], server_ip[2], server_ip[3],

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

// TODO: update this when we actually have random numbers
static NEXT_TRANSACTION_ID: AtomicU32 = AtomicU32::new(0xaabb0000);

pub fn get_transaction_id() -> u32 {
    NEXT_TRANSACTION_ID.fetch_add(1, Ordering::SeqCst)
}

static CURRENT_TRANSACTIONS: RwLock<BTreeSet<Transaction>> = RwLock::new(BTreeSet::new());

struct Transaction {
    xid: u32,
    mac: [u8; 6],
    state: TransactionState,
    blocked_task: TaskID
}

enum TransactionState {
    Discover,
    Request,
    Acknowledged,
}

pub fn handle_incoming_packet(data: &[u8]) {
    let packet_size = core::mem::size_of::<DhcpPacket>();
    if data.len() < packet_size {
        crate::kprintln!("Not long enough to be a DHCP packet!");
        return;
    }
    let packet = unsafe {
        &*(data.as_ptr() as *const DhcpPacket)
    };
    let options = &data[packet_size..];

    let mut subnet_mask: IPV4Address = IPV4Address([0, 0, 0, 0]);
    let mut router: IPV4Address = IPV4Address([0, 0, 0, 0]);
    let mut dhcp_server: IPV4Address = IPV4Address([0, 0, 0, 0]);
    let mut lease_time: u32 = 0;
    let mut dns_servers: Vec<IPV4Address> = Vec::new();
    let mut packet_type: u8 = 0;

    let mut options_cursor = 0;
    while options_cursor < options.len() {
        let tag = options[options_cursor];
        options_cursor += 1;
        match tag {
            // pad
            0x00 => (),
            // subnet mask
            0x01 => {
                let len = options[options_cursor] as usize;
                options_cursor += 1;

                subnet_mask[0] = options[options_cursor + 0];
                subnet_mask[1] = options[options_cursor + 1];
                subnet_mask[2] = options[options_cursor + 2];
                subnet_mask[3] = options[options_cursor + 3];

                options_cursor += len;
            },
            // router(s)
            0x03 => {
                let len = options[options_cursor] as usize;
                options_cursor += 1;

                router[0] = options[options_cursor + 0];
                router[1] = options[options_cursor + 1];
                router[2] = options[options_cursor + 2];
                router[3] = options[options_cursor + 3];

                // ignore the other routers

                options_cursor += len;
            },
            // dns servers
            0x06 => {
                let len = options[options_cursor] as usize;
                options_cursor += 1;

                let end = options_cursor + len;
                while options_cursor < end {
                    let mut ip = IPV4Address([
                        options[options_cursor + 0],
                        options[options_cursor + 1],
                        options[options_cursor + 2],
                        options[options_cursor + 3],
                    ]);
                    dns_servers.push(ip);
                    options_cursor += 4;
                }
            },
            // lease time
            0x33 => {
                let len = options[options_cursor] as usize;
                options_cursor += 1;

                lease_time =
                    ((options[options_cursor + 0] as u32) << 24) |
                    ((options[options_cursor + 1] as u32) << 16) |
                    ((options[options_cursor + 2] as u32) << 8) |
                    (options[options_cursor + 3] as u32);

                options_cursor += len;
            },
            // dhcp packet type
            0x35 => {
                let len = options[options_cursor] as usize;
                options_cursor += 1;

                packet_type = options[options_cursor];

                options_cursor += len;
            },
            // dhcp server
            0x36 => {
                let len = options[options_cursor] as usize;
                options_cursor += 1;

                dhcp_server[0] = options[options_cursor + 0];
                dhcp_server[1] = options[options_cursor + 1];
                dhcp_server[2] = options[options_cursor + 2];
                dhcp_server[3] = options[options_cursor + 3];

                options_cursor += len;
            },
            // end of options
            0xff => {
                options_cursor = options.len();
            },
            // unknown option
            _ => {
                // all options besides 0 and 0xff have a length field following
                // the tag
                let len = options[options_cursor] as usize;
                options_cursor += len + 1;
            },
        }
    }

    let transactions = CURRENT_TRANSACTIONS.write();
    let xid = packet.xid.to_le();

    let transaction_found = transactions
        .iter()
        .enumerate()
        .find(|(index, t)| {
            t.xid == xid
        });

    let (index, transaction) = match transaction_found {
        Some(pair) => pair,
        None => {
            crate::kprintln!("No DHCP transaction with xid {:#010X}", xid);
            return;
        },
    };

    match packet_type {
        // offer
        2 => {
            
        },
        // decline
        4 => {
        },
        // ack
        5 => {
        },

        _ => (),
    }
}
