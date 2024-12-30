use core::sync::atomic::{AtomicU32, Ordering};

use crate::net::socket::socket_broadcast;
use crate::task::id::TaskID;
use alloc::{collections::BTreeMap, vec::Vec};
use spin::{Once, RwLock};

use super::ethernet::HardwareAddress;
use super::get_net_device_by_mac;
use super::ip::IPV4Address;
use super::socket::{bind_socket, create_socket, SocketHandle, SocketPort, SocketProtocol};

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
        unsafe { core::slice::from_raw_parts(ptr, size) }
    }
}

pub fn discover_packet(mac: HardwareAddress, xid: u32) -> Vec<u8> {
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

    packet.chaddr[0..6].copy_from_slice(&*mac);

    let options: &[u8] = &[0x63, 0x82, 0x53, 0x63, 0x35, 0x01, 0x01, 0xff];

    let packet_size = core::mem::size_of::<DhcpPacket>();
    let total_len = packet_size + options.len();

    let mut packet_data = Vec::with_capacity(total_len);
    for _ in 0..total_len {
        packet_data.push(0);
    }

    packet_data.as_mut_slice()[..packet_size].copy_from_slice(packet.as_buffer());
    packet_data.as_mut_slice()[packet_size..].copy_from_slice(options);

    packet_data
}

pub fn request_packet(
    mac: HardwareAddress,
    server_ip: IPV4Address,
    requested_ip: IPV4Address,
    xid: u32,
) -> Vec<u8> {
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

    packet.chaddr[0..6].copy_from_slice(&*mac);

    let options: &[u8] = &[
        // magic cookie
        0x63,
        0x82,
        0x53,
        0x63,
        // DHCP request
        0x35,
        0x01,
        0x03,
        // requested IP
        0x32,
        0x04,
        requested_ip[0],
        requested_ip[1],
        requested_ip[2],
        requested_ip[3],
        // server IP
        0x36,
        0x04,
        server_ip[0],
        server_ip[1],
        server_ip[2],
        server_ip[3],
        0xff,
    ];

    let packet_size = core::mem::size_of::<DhcpPacket>();
    let total_len = packet_size + options.len();

    let mut packet_data = Vec::with_capacity(total_len);
    for _ in 0..total_len {
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

static CURRENT_TRANSACTIONS: RwLock<BTreeMap<u32, Transaction>> = RwLock::new(BTreeMap::new());

struct Transaction {
    mac: HardwareAddress,
    state: TransactionState,
}

enum TransactionState {
    Discover,
    Request,
}

static DHCP_SOCKET: Once<SocketHandle> = Once::new();

fn get_dhcp_socket() -> SocketHandle {
    *DHCP_SOCKET.call_once(|| {
        let socket = create_socket(SocketProtocol::UDP);
        bind_socket(
            socket,
            IPV4Address([0, 0, 0, 0]),
            SocketPort::new(68),
            IPV4Address([255, 255, 255, 255]),
            SocketPort::new(67),
        )
        .unwrap();
        socket
    })
}

pub fn start_dhcp_transaction(blocked_task: TaskID, mac: HardwareAddress) {
    crate::kprintln!("Start DHCP transaction");
    let xid = get_transaction_id();
    let transaction = Transaction {
        mac,
        state: TransactionState::Discover,
    };

    CURRENT_TRANSACTIONS.write().insert(xid, transaction);

    let dhcp_data = discover_packet(mac, xid);
    socket_broadcast(get_dhcp_socket(), &dhcp_data).unwrap();
}

pub fn handle_incoming_packet(data: &[u8]) {
    let packet_size = core::mem::size_of::<DhcpPacket>();
    if data.len() < packet_size {
        return;
    }
    let packet = unsafe { &*(data.as_ptr() as *const DhcpPacket) };
    let options = &data[packet_size..];

    let mut subnet_mask: IPV4Address = IPV4Address([0, 0, 0, 0]);
    let mut router: IPV4Address = IPV4Address([0, 0, 0, 0]);
    let mut dhcp_server: IPV4Address = IPV4Address([0, 0, 0, 0]);
    let mut lease_time: u32 = 0;
    let mut dns_servers: Vec<IPV4Address> = Vec::new();
    let mut packet_type: u8 = 0;

    let mut options_cursor = 4;

    if options.len() < 4 || options[0..4] != [0x63, 0x82, 0x53, 0x63] {
        return;
    }

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
            }
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
            }
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
            }
            // lease time
            0x33 => {
                let len = options[options_cursor] as usize;
                options_cursor += 1;

                lease_time = ((options[options_cursor + 0] as u32) << 24)
                    | ((options[options_cursor + 1] as u32) << 16)
                    | ((options[options_cursor + 2] as u32) << 8)
                    | (options[options_cursor + 3] as u32);

                options_cursor += len;
            }
            // dhcp packet type
            0x35 => {
                let len = options[options_cursor] as usize;
                options_cursor += 1;

                packet_type = options[options_cursor];

                options_cursor += len;
            }
            // dhcp server
            0x36 => {
                let len = options[options_cursor] as usize;
                options_cursor += 1;

                dhcp_server[0] = options[options_cursor + 0];
                dhcp_server[1] = options[options_cursor + 1];
                dhcp_server[2] = options[options_cursor + 2];
                dhcp_server[3] = options[options_cursor + 3];

                options_cursor += len;
            }
            // end of options
            0xff => {
                options_cursor = options.len();
            }
            // unknown option
            _ => {
                // all options besides 0 and 0xff have a length field following
                // the tag
                let len = options[options_cursor] as usize;
                options_cursor += len + 1;
            }
        }
    }

    let xid = packet.xid.to_be();
    match packet_type {
        // offer
        2 => {
            let mac = match CURRENT_TRANSACTIONS.write().get_mut(&xid) {
                Some(t) => {
                    t.state = TransactionState::Request;
                    t.mac
                }
                None => {
                    return;
                }
            };
            let request = request_packet(mac, packet.siaddr, packet.yiaddr, xid);
            socket_broadcast(get_dhcp_socket(), &request).unwrap();
        }
        // decline
        4 => {
            CURRENT_TRANSACTIONS.write().remove(&xid);
        }
        // ack
        5 => {
            let mac = match CURRENT_TRANSACTIONS.write().remove(&xid) {
                Some(t) => t.mac,
                None => return,
            };
            match get_net_device_by_mac(mac) {
                Some(netdev) => {
                    netdev.ip.write().replace(packet.yiaddr);
                    crate::kprintln!("Net device now has IP {:}", packet.yiaddr);
                }
                None => (),
            }
        }

        _ => (),
    }
}
