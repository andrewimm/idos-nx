use crate::{
    collections::SlotList,
    io::handle::{Handle, PendingHandleOp},
    net::udp::create_datagram,
    task::actions::handle::{
        add_handle_to_notify_queue, create_file_handle, create_notify_queue, handle_op_open,
        handle_op_read, handle_op_write, wait_on_notify,
    },
};

use alloc::vec::Vec;

use super::{
    arp::{ARPPacket, ARPTable},
    dhcp::DHCPState,
    ethernet::{EthernetFrameHeader, HardwareAddress, ETHERTYPE_ARP, ETHERTYPE_IP},
    ip::{IPProtocolType, IPV4Header},
    udp::UDPHeader,
};
use super::{ip::IPV4Address, packet::PacketHeader};

struct RegisteredNetDevice {
    handle: Handle,
    mac: HardwareAddress,
    is_open: bool,
    current_op: PendingHandleOp,
    read_buffer: Vec<u8>,
    arp_table: ARPTable,
    dhcp_state: DHCPState,
}

impl RegisteredNetDevice {
    pub fn new(device_path: &str, mac: HardwareAddress, notify_queue: Handle) -> Self {
        let handle = create_file_handle();
        add_handle_to_notify_queue(notify_queue, handle);

        let mut read_buffer = Vec::with_capacity(1024);
        for _ in 0..1024 {
            read_buffer.push(0);
        }

        Self {
            handle,
            mac,
            is_open: false,
            current_op: handle_op_open(handle, device_path),
            read_buffer,
            arp_table: ARPTable::new(),
            dhcp_state: DHCPState::new(mac),
        }
    }

    /// This method should be called whenever the network interface has
    /// completed a read into this struct's read buffer. It will inspect the
    /// ethernet frame and process its contents according to
    pub fn process_read_buffer(&mut self) {
        if let Some(frame) = EthernetFrameHeader::try_from_u8_buffer(&self.read_buffer) {
            let offset = EthernetFrameHeader::get_size();
            match frame.get_ethertype() {
                // if it's an ARP response, process it with the device's ARP
                // state
                ETHERTYPE_ARP => self.handle_arp_packet(offset),
                // if it's an IP packet, it may be UDP or TCP and needs to
                // be handled by the appropriate socket
                ETHERTYPE_IP => self.handle_ip_packet(frame.src_mac, offset),
                _ => (),
            }
        }
    }

    /// Sending data to another device on the network requires knowing the MAC
    /// address of that node. If the MAC address is not already known, an ARP
    /// request will be sent. Because this is async, it requires all raw message
    /// sends to be async as well. Those async tasks will store a waker which
    /// can be resumed when the ARP resolution is complete.
    /// ARP responses can also be unrequested, in which case the MAC address
    /// will be cached in case it needs to be used later.
    ///
    /// When this method is called, it is assumed that read_buffer contains a
    /// valid Ethernet Frame with an ARP packet payload.
    fn handle_arp_packet(&mut self, read_offset: usize) {
        let arp = ARPPacket::try_from_u8_buffer(&self.read_buffer[read_offset..]).unwrap();
        let was_pending = self
            .arp_table
            .resolve_lookup(arp.source_protocol_addr, arp.source_hardware_addr);
        if was_pending {
            // if any async tasks were blocked on this, wake their futures
        }
    }

    /// When an IP packet is received, determine the type (UDP / TCP) and then
    /// determine if its destination is a valid open socket. Similar to
    /// ARP resolution, this method assumes that read_buffer contains a valid
    /// Ethernet Frame containing an IP packet.
    /// If it's UDP, do a special-case check to see if it's a DHCP packet.
    /// DHCP packets are sent to port 68, and are handled with the DHCP state on
    /// the current net device. Fetching the current IP is an async request, so
    /// DHCP resolution may also have an associated async waker.
    pub fn handle_ip_packet(&mut self, src_mac: HardwareAddress, read_offset: usize) {
        let ip_header = IPV4Header::try_from_u8_buffer(&self.read_buffer[read_offset..]).unwrap();
        let payload_offset = read_offset + IPV4Header::get_size();
        let total_length = ip_header.total_length.to_be() as usize;
        let payload = &self.read_buffer[payload_offset..total_length];

        if ip_header.protocol == IPProtocolType::TCP {
            // handle TCP packet
        } else if ip_header.protocol == IPProtocolType::UDP {
            let udp_header = match UDPHeader::try_from_u8_buffer(payload) {
                Some(header) => header,
                None => return,
            };
            if udp_header.dest_port.to_be() == 68 {
                // special handling for DHCP
                let udp_payload = &payload[UDPHeader::get_size()..];
                crate::kprintln!("LOOK A DHCP PACKET");
                if let Some(dhcp_response) = self.dhcp_state.handle_dhcp_packet(udp_payload) {
                    // if the DHCP state machine has a response, send it
                    let eth_header =
                        EthernetFrameHeader::new_ipv4(self.mac, HardwareAddress::broadcast());
                    let ip_packet = create_datagram(
                        IPV4Address([0; 4]),
                        68,
                        IPV4Address([255; 4]),
                        67,
                        &dhcp_response,
                    );
                    self.send_raw(eth_header, &ip_packet);
                }
            } else {
            }
        }
    }

    pub fn send_raw(&self, eth_header: EthernetFrameHeader, payload: &[u8]) -> PendingHandleOp {
        let mut total_frame = Vec::with_capacity(EthernetFrameHeader::get_size() + payload.len());
        total_frame.extend_from_slice(eth_header.as_u8_buffer());
        total_frame.extend(payload);

        handle_op_write(self.handle, &total_frame)
    }
}

pub fn net_stack_resident() -> ! {
    // this notify queue will be used to listen for all network devices
    // each time a new network device is registered, it will be opened and the
    // handle will be attached to this queue
    let notify = create_notify_queue();

    let mut network_devices: SlotList<RegisteredNetDevice> = SlotList::new();

    // TODO: move this out to an external call
    let my_mac = HardwareAddress([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    network_devices.insert(RegisteredNetDevice::new("DEV:\\ETH", my_mac, notify));

    {
        let packet = super::dhcp::DHCPPacket::discovery_packet(my_mac, 0xaabb0000);
        let mut total_frame = Vec::with_capacity(EthernetFrameHeader::get_size() + packet.len());
        let eth_header = EthernetFrameHeader::new_ipv4(my_mac, HardwareAddress::broadcast());
        total_frame.extend_from_slice(eth_header.as_u8_buffer());
        total_frame.extend(packet);

        /*crate::task::actions::handle::handle_op_write(
            network_devices.get(0).unwrap().handle,
            &total_frame,
        )
        .wait_for_completion();*/
    }

    // let the init task know that the network stack is ready
    let response_writer = Handle::new(0);
    handle_op_write(response_writer, &[1]);

    // each time a device is registered, open it and add its handle to the
    // notify queue
    // also add a pending read op, which can be read from within the loop
    //
    // each network device also has an associated state machine which stores
    // its own ARP, DHCP, and socket states

    loop {
        // For each device, check the read op
        crate::kprintln!(" ~ ~ NETWAKE");
        for net_dev in network_devices.iter_mut() {
            if !net_dev.current_op.is_complete() {
                continue;
            }
            if !net_dev.is_open {
                // If the device was not opened yet, the completed op must be
                // the initial open op. Mark the device as opened and begin
                // reading from the device.
                net_dev.is_open = true;
                net_dev.current_op = handle_op_read(net_dev.handle, &mut net_dev.read_buffer, 0);
                continue;
            }
            // if data is ready, inspect the packet
            // TODO: Implement error handling
            let len = net_dev.current_op.get_result().unwrap() as usize;
            if len == 0 {
                net_dev.current_op = handle_op_read(net_dev.handle, &mut net_dev.read_buffer, 0);
                continue;
            }

            net_dev.process_read_buffer();

            for i in 0..net_dev.read_buffer.len() {
                net_dev.read_buffer[i] = 0;
            }
            net_dev.current_op = handle_op_read(net_dev.handle, &mut net_dev.read_buffer, 0);
        }

        // check the task queue for external requests

        // block on notify queue
        wait_on_notify(notify, Some(1000));
    }
}

// async/await primitives, probably put this in another module soon
