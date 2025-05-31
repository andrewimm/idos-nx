use core::sync::atomic::Ordering;

use crate::{
    collections::SlotList,
    io::handle::Handle,
    net::udp::create_datagram,
    task::actions::{
        handle::create_file_handle,
        io::{append_io_op, write_sync},
        sync::{block_on_wake_set, create_wake_set},
    },
};

use alloc::{boxed::Box, collections::VecDeque, string::String, vec::Vec};
use idos_api::io::{AsyncOp, ASYNC_OP_OPEN, ASYNC_OP_READ, ASYNC_OP_WRITE};

use super::{
    arp::{ARPPacket, ARPTable},
    dhcp::DHCPState,
    ethernet::{EthernetFrameHeader, HardwareAddress, ETHERTYPE_ARP, ETHERTYPE_IP},
    ip::{IPProtocolType, IPV4Header},
    udp::UDPHeader,
};
use super::{ip::IPV4Address, packet::PacketHeader};

use spin::Mutex;

struct RegisteredNetDevice {
    handle: Handle,
    wake_set: Handle,
    mac: HardwareAddress,
    is_open: bool,
    current_read: Box<AsyncOp>,
    current_writes: VecDeque<(Vec<u8>, Box<AsyncOp>)>,
    read_buffer: Vec<u8>,
    arp_table: ARPTable,
    dhcp_state: DHCPState,
}

impl RegisteredNetDevice {
    pub fn new(device_path: &str, mac: HardwareAddress, wake_set: Handle) -> Self {
        let handle = create_file_handle();

        let mut read_buffer = Vec::with_capacity(1024);
        for _ in 0..1024 {
            read_buffer.push(0);
        }

        let current_read = Box::new(AsyncOp::new(
            ASYNC_OP_OPEN,
            device_path.as_ptr() as u32,
            device_path.len() as u32,
            0,
        ));
        let _ = append_io_op(handle, &current_read, Some(wake_set));

        Self {
            handle,
            wake_set,
            mac,
            is_open: false,
            current_read,
            current_writes: VecDeque::new(),
            read_buffer,
            arp_table: ARPTable::new(),
            dhcp_state: DHCPState::new(mac),
        }
    }

    pub fn init_dhcp(&mut self) {
        if let Some(packet) = self.dhcp_state.start_transaction() {
            self.current_writes.push_back(self.dhcp_broadcast(&packet));
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
    pub fn handle_ip_packet(&mut self, _src_mac: HardwareAddress, read_offset: usize) {
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
                    self.current_writes
                        .push_back(self.dhcp_broadcast(&dhcp_response));
                }
            } else {
            }
        }
    }

    /// Special handling for sending DHCP payloads through ethernet broadcasts.
    /// We don't open a true socket for DHCP navigation. We just fake it.
    pub fn dhcp_broadcast(&self, payload: &[u8]) -> (Vec<u8>, Box<AsyncOp>) {
        let eth_header = EthernetFrameHeader::new_ipv4(self.mac, HardwareAddress::broadcast());
        let ip_packet =
            create_datagram(IPV4Address([0; 4]), 68, IPV4Address([255; 4]), 67, payload);
        self.send_raw(eth_header, &ip_packet)
    }

    pub fn send_raw(
        &self,
        eth_header: EthernetFrameHeader,
        payload: &[u8],
    ) -> (Vec<u8>, Box<AsyncOp>) {
        let mut total_frame = Vec::with_capacity(EthernetFrameHeader::get_size() + payload.len());
        total_frame.extend_from_slice(eth_header.as_u8_buffer());
        total_frame.extend(payload);

        let async_op = Box::new(AsyncOp::new(
            ASYNC_OP_WRITE,
            total_frame.as_ptr() as u32,
            total_frame.len() as u32,
            0,
        ));
        let _ = append_io_op(self.handle, &async_op, Some(self.wake_set));

        // pass the vec so it can be stored, and not immediately dropped
        (total_frame, async_op)
    }
}

pub enum NetRequest {
    RegisterDevice(String, HardwareAddress),
    GetIP,
    SocketBind,
    SocketAccept,
    SocketRead,
    SocketWrite,
    SocketClose,
}

static NET_STACK_REQUESTS: Mutex<VecDeque<NetRequest>> = Mutex::new(VecDeque::new());

pub fn register_network_device(name: &str, mac: [u8; 6]) {
    NET_STACK_REQUESTS
        .lock()
        .push_back(NetRequest::RegisterDevice(
            String::from(name),
            HardwareAddress(mac),
        ));
}

pub fn net_stack_resident() -> ! {
    // this wake set will be used to listen for all network devices
    // each time a new network device is registered, it will be passed in and
    // the io operations will notify it
    let wake_set = create_wake_set();

    let mut network_devices: SlotList<RegisteredNetDevice> = SlotList::new();

    // let the init task know that the network stack is ready
    let response_writer = Handle::new(0);
    let _ = write_sync(response_writer, &[1], 0);

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
            // clear out any completed writes
            loop {
                let pop = if let Some((_, pending_write)) = net_dev.current_writes.front() {
                    pending_write.is_complete()
                } else {
                    false
                };
                if pop {
                    net_dev.current_writes.pop_front();
                } else {
                    break;
                }
            }
            // process the pending read, if it's ready
            if !net_dev.current_read.is_complete() {
                continue;
            }
            if !net_dev.is_open {
                // If the device was not opened yet, the completed op must be
                // the initial open op. Mark the device as opened and begin
                // reading from the device.
                net_dev.is_open = true;
                net_dev.init_dhcp();

                net_dev.current_read = Box::new(AsyncOp::new(
                    ASYNC_OP_READ,
                    net_dev.read_buffer.as_mut_ptr() as u32,
                    net_dev.read_buffer.len() as u32,
                    0,
                ));
                let _ = append_io_op(net_dev.handle, &net_dev.current_read, Some(wake_set));
                continue;
            }
            // if data is ready, inspect the packet
            let read_result = net_dev.current_read.return_value.load(Ordering::SeqCst);
            if read_result & 0x80000000 != 0 {
                // TODO: implement error handling
            } else {
                let len = read_result as usize & 0x7fffffff;
                if len > 0 {
                    net_dev.process_read_buffer();

                    for i in 0..net_dev.read_buffer.len() {
                        net_dev.read_buffer[i] = 0;
                    }
                } else {
                    crate::kprintln!("NET RESPONSE ZERO LENGTH");
                }
            }
            net_dev.current_read = Box::new(AsyncOp::new(
                ASYNC_OP_READ,
                net_dev.read_buffer.as_mut_ptr() as u32,
                net_dev.read_buffer.len() as u32,
                0,
            ));
            let _ = append_io_op(net_dev.handle, &net_dev.current_read, Some(wake_set));
        }

        // check the task queue for external requests
        // External async requests include:
        //  - Register network device by name + MAC
        //  - Socket accept / send / receive / close
        //  - IP lookup (async because DHCP may not have been established yet)
        if let Some(mut queue) = NET_STACK_REQUESTS.try_lock() {
            while let Some(req) = queue.pop_front() {
                match req {
                    NetRequest::RegisterDevice(name, mac) => {
                        network_devices.insert(RegisteredNetDevice::new(&name, mac, wake_set));
                    }
                    _ => {}
                }
            }
        }

        block_on_wake_set(wake_set, None);
    }
}

// async/await primitives, probably put this in another module soon
