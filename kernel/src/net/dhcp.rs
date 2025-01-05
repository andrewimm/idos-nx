use crate::net::socket::socket_broadcast;
use crate::time::system::Timestamp;
use alloc::vec::Vec;
use spin::Once;

use super::ethernet::HardwareAddress;
use super::ip::IPV4Address;
use super::packet::PacketHeader;
use super::socket::{bind_socket, create_socket, SocketHandle, SocketPort, SocketProtocol};

#[repr(C, packed)]
pub struct DHCPPacket {
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

impl DHCPPacket {
    /// Create a DHCP DISCOVER packet, used for finding the DHCP server
    pub fn discovery_packet(mac: HardwareAddress, xid: u32) -> Vec<u8> {
        let mut packet = Self {
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

        let options: &[u8] = &[
            0x63, 0x82, 0x53, 0x63, // magic cookie
            0x35, 0x01, 0x01, // DHCP discover
            0xff, // end list
        ];

        let packet_size = core::mem::size_of::<Self>();
        let total_len = packet_size + options.len();

        let mut packet_data = Vec::with_capacity(total_len);
        for _ in 0..total_len {
            packet_data.push(0);
        }

        packet_data.as_mut_slice()[..packet_size].copy_from_slice(packet.as_u8_buffer());
        packet_data.as_mut_slice()[packet_size..].copy_from_slice(options);

        packet_data
    }

    /// Create a DHCP REQUEST packet, used for requesting an IP address
    pub fn request_packet(
        mac: HardwareAddress,
        server_ip: IPV4Address,
        requested_ip: IPV4Address,
        xid: u32,
    ) -> Vec<u8> {
        let mut packet = Self {
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
            0x63,
            0x82,
            0x53,
            0x63, // magic cookie
            0x35,
            0x01,
            0x03, // DHCP request operation
            0x32,
            0x04,
            requested_ip[0],
            requested_ip[1],
            requested_ip[2],
            requested_ip[3], // requested IP
            0x36,
            0x04,
            server_ip[0],
            server_ip[1],
            server_ip[2],
            server_ip[3], // server IP
            0xff,         // end list
        ];

        let packet_size = core::mem::size_of::<Self>();
        let total_len = packet_size + options.len();

        let mut packet_data = Vec::with_capacity(total_len);
        for _ in 0..total_len {
            packet_data.push(0);
        }

        packet_data.as_mut_slice()[..packet_size].copy_from_slice(packet.as_u8_buffer());
        packet_data.as_mut_slice()[packet_size..].copy_from_slice(options);

        packet_data
    }
}

impl PacketHeader for DHCPPacket {}

#[derive(Copy, Clone)]
pub enum IPAddressState {
    /// No IP established
    Unknown,
    /// A transaction is in progress. The u32 is the transaction ID
    Pending(u32),
    /// An IP has been established
    Established(IPV4Address),
    /// The IP has expired. A new transcaction requesting the previous address
    /// again should be initiated
    Expired(IPV4Address),
}

/// This struct holds an internal state machine for tracking the IP of a
/// network interface. If no IP is established, it can initiate a DHCP
/// transaction.
pub struct DHCPState {
    mac: HardwareAddress,
    ip_address: IPAddressState,
    pub subnet_mask: IPV4Address,
    pub dhcp_server: IPV4Address,
    expires_at: Timestamp,
}

impl DHCPState {
    pub fn new(mac: HardwareAddress) -> Self {
        Self {
            mac,
            ip_address: IPAddressState::Unknown,
            subnet_mask: IPV4Address::default(),
            dhcp_server: IPV4Address::default(),
            expires_at: Timestamp(0),
        }
    }

    pub fn get_current_ip_state(&mut self) -> IPAddressState {
        if let IPAddressState::Established(ip) = self.ip_address {
            if self.expires_at < Timestamp::now() {
                self.ip_address = IPAddressState::Expired(ip);
            }
        }
        self.ip_address
    }

    pub fn start_transaction(&mut self) -> Option<Vec<u8>> {
        match self.get_current_ip_state() {
            IPAddressState::Unknown => {
                let xid = 0xaabb0000;
                self.ip_address = IPAddressState::Pending(xid);
                Some(DHCPPacket::discovery_packet(self.mac, xid))
            }
            IPAddressState::Pending(_) => None,
            IPAddressState::Established(_) => None,
            IPAddressState::Expired(ip) => {
                let xid = 0xaabb0000;
                let packet = DHCPPacket::request_packet(self.mac, self.dhcp_server, ip, xid);
                self.ip_address = IPAddressState::Pending(xid);
                Some(packet)
            }
        }
    }

    pub fn handle_dhcp_packet(&mut self, raw: &[u8]) -> Option<Vec<u8>> {
        let packet = match DHCPPacket::try_from_u8_buffer(raw) {
            Some(packet) => packet,
            None => return None,
        };

        let packet_size = DHCPPacket::get_size();
        let options = &raw[packet_size..];

        let mut subnet_mask: IPV4Address = IPV4Address([0, 0, 0, 0]);
        let mut router: IPV4Address = IPV4Address([0, 0, 0, 0]);
        let mut dhcp_server: IPV4Address = IPV4Address([0, 0, 0, 0]);
        let mut lease_time: u32 = 0;
        let mut dns_servers: Vec<IPV4Address> = Vec::new();
        let mut packet_type: u8 = 0;

        let mut options_cursor = 4;

        if options.len() < 4 || options[0..4] != [0x63, 0x82, 0x53, 0x63] {
            return None;
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
                        let ip = IPV4Address([
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
            // The DHCP server offers an IP, which the current device can
            // now request. Generate a request packet that the owning net device
            // can send.
            2 => {
                crate::kprintln!("DHCP OFFER");

                let response =
                    DHCPPacket::request_packet(self.mac, dhcp_server, packet.yiaddr, xid);
                return Some(response);
            }
            // decline
            // The DHCP server has declined the request. The DHCP request should
            // now fail.
            4 => {}
            // ack
            // The DHCP request was approved. The IP address can now be
            // associated with the network device's MAC.
            5 => {
                crate::kprintln!("DHCP ACK");
                self.dhcp_server = dhcp_server;
                self.ip_address = IPAddressState::Established(packet.yiaddr);
                self.expires_at = Timestamp::now() + lease_time;
            }
            _ => (),
        }

        None
    }
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

pub fn start_dhcp_transaction(mac: HardwareAddress) {
    crate::kprintln!("Start DHCP transaction");
    let dhcp_data = DHCPPacket::discovery_packet(mac, 0xaabb0000);
    socket_broadcast(get_dhcp_socket(), &dhcp_data).unwrap();
}
