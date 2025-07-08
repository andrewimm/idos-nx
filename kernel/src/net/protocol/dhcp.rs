use alloc::vec::Vec;

use crate::time::system::Timestamp;

use super::super::hardware::HardwareAddress;
use super::ipv4::Ipv4Address;
use super::packet::PacketHeader;

#[repr(C, packed)]
pub struct DhcpPacket {
    /// DHCP message type; 1 for request, 2 for reply
    pub op: u8,
    /// Hardware address type; almost always 1 for Ethernet
    pub htype: u8,
    /// Hardware address length; 6 for Ethernet
    pub hlen: u8,
    /// Hops; used for relay
    pub hops: u8,
    /// Transaction ID; used to match requests and replies, should be unique
    pub xid: u32,
    /// Seconds elapsed since the client started the DHCP process, filled by client
    pub secs: u16,
    /// Flags; used for broadcast requests and nothing else
    pub flags: u16,
    /// Client IP address; filled by client if it is already bound
    pub ciaddr: Ipv4Address,
    /// Your IP address; filled by server in reply
    pub yiaddr: Ipv4Address,
    /// Server IP address; filled by server in reply
    pub siaddr: Ipv4Address,
    /// Gateway IP address; filled by server in reply if used
    pub giaddr: Ipv4Address,
    /// Client hardware address; filled by client, used to identify the client
    pub chaddr: [u8; 16],
    /// Server host name; optional, filled by client, null-terminated string
    pub sname: [u8; 64],
    /// Boot file name; can't imagine this being used in the OS
    pub file: [u8; 128],
}

impl DhcpPacket {
    const REQUEST_OP: u8 = 1;
    const REPLY_OP: u8 = 2;

    /// Create a DHCP DISCOVER request packet, used for finding the DHCP server
    pub fn discover(mac: HardwareAddress, xid: u32) -> Vec<u8> {
        let mut packet = Self {
            op: Self::REQUEST_OP,
            htype: 1,
            hlen: 6,
            hops: 0,
            xid,
            secs: 0,
            flags: 0, // send response as unicast
            ciaddr: Ipv4Address::default(),
            yiaddr: Ipv4Address::default(),
            siaddr: Ipv4Address::default(),
            giaddr: Ipv4Address::default(),
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

        let header_size = core::mem::size_of::<Self>();
        let total_len = header_size + options.len();

        let mut packet_data = Vec::with_capacity(total_len);
        for _ in 0..total_len {
            packet_data.push(0);
        }

        packet_data.as_mut_slice()[..header_size].copy_from_slice(packet.as_u8_buffer());
        packet_data.as_mut_slice()[header_size..].copy_from_slice(options);

        packet_data
    }

    /// Create a DHCP REQUEST packet, used for requesting an IP address
    pub fn request(
        mac: HardwareAddress,
        server_ip: Ipv4Address,
        requested_ip: Ipv4Address,
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

            ciaddr: Ipv4Address([0, 0, 0, 0]),
            yiaddr: Ipv4Address([0, 0, 0, 0]),
            siaddr: server_ip,
            giaddr: Ipv4Address([0, 0, 0, 0]),

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

        let header_size = core::mem::size_of::<Self>();
        let total_len = header_size + options.len();

        let mut packet_data = Vec::with_capacity(total_len);
        for _ in 0..total_len {
            packet_data.push(0);
        }

        packet_data.as_mut_slice()[..header_size].copy_from_slice(packet.as_u8_buffer());
        packet_data.as_mut_slice()[header_size..].copy_from_slice(options);

        packet_data
    }
}

impl PacketHeader for DhcpPacket {}

pub struct DhcpState {
    /// Stores the current resolution state of the local IP address
    pub local_ip: IpResolution,
    /// IP of the DHCP server
    pub server_ip: Ipv4Address,
    /// Gateway IP for exiting the local network.
    /// When a packet needs to be sent outside the local network, the MAC
    /// address of the gateway is used for the outgoing packet.
    pub gateway_ip: Ipv4Address,
    /// Subnet mask for identifying IPs on the local network
    pub subnet_mask: Ipv4Address,
}

/// State of DHCP resolution request
pub enum IpResolution {
    /// The process has not initiated yet
    Unbound,
    /// A request is in progress, the xid is stored in the tuple
    Progress(u32),
    /// The request has completed, the IP and expiration time are stored in the tuple.
    /// If the current time is actually greater than the expiration timestamp,
    /// the IP should be considered expired.
    Bound(Ipv4Address, Timestamp),
    /// A renew request is in progress. The requested IP and xid are stored in the tuple.
    Renewing(Ipv4Address, u32),
}

impl DhcpState {
    pub fn new() -> Self {
        DhcpState {
            local_ip: IpResolution::Unbound,
            server_ip: Ipv4Address::default(),
            gateway_ip: Ipv4Address::default(),
            subnet_mask: Ipv4Address::default(),
        }
    }

    /// Determine if an IP address is on the local network, based on the subnet mask
    pub fn is_local(&self, other_ip: Ipv4Address) -> bool {
        unimplemented!();
    }

    pub fn get_local_ip(&self) -> Option<Ipv4Address> {
        match self.local_ip {
            IpResolution::Bound(ip, _) => Some(ip),
            _ => None,
        }
    }
}
