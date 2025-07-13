use crate::{
    collections::SlotList,
    executor::{Executor, WaitForEvent, WakerRegistry},
    io::handle::Handle,
    log::TaggedLogger,
    task::actions::{
        io::write_sync,
        sync::{block_on_wake_set, create_wake_set},
    },
};

use alloc::{boxed::Box, collections::VecDeque, string::String, sync::Arc, vec::Vec};

use super::{
    hardware::HardwareAddress,
    netdevice::{NetDevice, NetEvent},
    protocol::{
        arp::ArpPacket,
        dhcp::{DhcpPacket, IpResolution},
        ethernet::EthernetFrameHeader,
        ipv4::Ipv4Address,
        packet::PacketHeader,
        udp::create_datagram,
    },
};

use spin::{Mutex, RwLock};

pub const LOGGER: TaggedLogger = TaggedLogger::new("NET", 92);

/*
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
*/

pub enum NetRequest {
    RegisterDevice(String, HardwareAddress),
    GetIp,
    GetMacForIp(Ipv4Address),
    SendUdp(u16, Ipv4Address, u16, Vec<u8>),
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

pub fn get_ip() {
    NET_STACK_REQUESTS.lock().push_back(NetRequest::GetIp);
}

pub fn get_mac_for_ip(ip: Ipv4Address) {
    NET_STACK_REQUESTS
        .lock()
        .push_back(NetRequest::GetMacForIp(ip));
}

pub fn send_udp(source_port: u16, destination: Ipv4Address, dest_port: u16, payload: Vec<u8>) {
    NET_STACK_REQUESTS.lock().push_back(NetRequest::SendUdp(
        source_port,
        destination,
        dest_port,
        payload,
    ));
}

pub fn net_stack_resident() -> ! {
    // this wake set will be used to listen for all network devices
    // each time a new network device is registered, it will be passed in and
    // the io operations will notify it
    let wake_set = create_wake_set();

    let mut network_devices: SlotList<(Executor<NetEvent>, Arc<RwLock<NetDevice>>)> =
        SlotList::new();

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
        // check the task queue for external requests
        // External async requests include:
        //  - Register network device by name + MAC
        //  - Socket accept / send / receive / close
        //  - IP lookup (async because DHCP may not have been established yet)
        if let Some(mut queue) = NET_STACK_REQUESTS.try_lock() {
            while let Some(req) = queue.pop_front() {
                match req {
                    NetRequest::RegisterDevice(name, mac) => {
                        LOGGER.log(format_args!("Register Device {}", name));
                        network_devices.insert((
                            Executor::<NetEvent>::new(),
                            Arc::new(RwLock::new(NetDevice::new(&name, mac, wake_set))),
                        ));
                    }
                    NetRequest::GetIp => {
                        let (executor, active_device) = network_devices.get_mut(0).unwrap();
                        let device = active_device.clone();
                        let waker_reg = executor.waker_registry();
                        executor.spawn(async move {
                            let _ = get_local_ip(device, waker_reg).await;
                        });
                    }
                    NetRequest::GetMacForIp(ip) => {
                        let (executor, active_device) = network_devices.get_mut(0).unwrap();
                        let device = active_device.clone();
                        let waker_reg = executor.waker_registry();
                        executor.spawn(async move {
                            if let Some(mac) = resolve_ip_to_mac(ip, device, waker_reg).await {
                                LOGGER.log(format_args!("Resolved {} to MAC {}", ip, mac));
                            } else {
                                LOGGER.log(format_args!("Failed to resolve IP {}", ip));
                            }
                        });
                    }
                    NetRequest::SendUdp(source_port, dest, dest_port, payload) => {
                        let (executor, active_device) = network_devices.get_mut(0).unwrap();
                        let device = active_device.clone();
                        let waker_reg = executor.waker_registry();
                        executor.spawn(async move {
                            match send_udp_packet(
                                dest,
                                source_port,
                                dest_port,
                                payload,
                                device,
                                waker_reg,
                            )
                            .await
                            {
                                Ok(_) => {}
                                Err(_) => {
                                    LOGGER.log(format_args!("Failed to send UDP packet"));
                                }
                            }
                        });
                    }
                }
            }
        }

        // For each device, check the read op
        for (executor, net_dev_lock) in network_devices.iter_mut() {
            let read_event = {
                let mut net_dev = net_dev_lock.write();
                net_dev.clear_completed_writes();
                net_dev.process_read_result()
            };
            if let Some(event) = read_event {
                executor.notify_event(&event);
            }
            executor.poll_tasks();
        }

        block_on_wake_set(wake_set, None);
    }
}

async fn get_local_ip(
    net_dev_lock: Arc<RwLock<NetDevice>>,
    waker_registry: WakerRegistry<NetEvent>,
) -> Option<Ipv4Address> {
    let is_open = net_dev_lock.read().is_open;
    if !is_open {
        WaitForEvent::new(NetEvent::LinkEstablished, waker_registry.clone()).await;
    }
    let resolved_ip = net_dev_lock.read().dhcp_state.local_ip.clone();
    let xid = match resolved_ip {
        IpResolution::Bound(ip, _expiration) => {
            return Some(ip);
        }
        IpResolution::Unbound => {
            // never initialized before, run the whole process
            LOGGER.log(format_args!("INIT DHCP"));
            // TODO: random number generator
            let xid = 0xabcd;
            net_dev_lock.write().init_dhcp(xid);
            // after sending the broadcast, wait for an offer
            WaitForEvent::new(NetEvent::DhcpOffer(xid), waker_registry.clone()).await;
            xid
        }
        IpResolution::Progress(xid) => xid,
        IpResolution::Renewing(_, xid) => xid,
    };
    WaitForEvent::new(NetEvent::DhcpAck(xid), waker_registry.clone()).await;

    let final_state = net_dev_lock.read().dhcp_state.local_ip.clone();
    match final_state {
        IpResolution::Bound(ip, _expiration) => Some(ip),
        _ => None,
    }
}

async fn resolve_ip_to_mac(
    target_ip: Ipv4Address,
    net_dev_lock: Arc<RwLock<NetDevice>>,
    waker_registry: WakerRegistry<NetEvent>,
) -> Option<HardwareAddress> {
    let is_open = net_dev_lock.read().is_open;
    if !is_open {
        WaitForEvent::new(NetEvent::LinkEstablished, waker_registry.clone()).await;
    }
    if let Some(mac) = net_dev_lock.read().known_arp.get(&target_ip).cloned() {
        return Some(mac);
    }

    // If not known, send an ARP request and wait for a response
    let local_ip = get_local_ip(net_dev_lock.clone(), waker_registry.clone()).await?;
    {
        let mut net_dev = net_dev_lock.write();
        let local_mac = net_dev.mac;
        let arp_request = ArpPacket::request(local_mac, local_ip, target_ip);
        let eth_frame = EthernetFrameHeader::broadcast_arp(local_mac);
        let write = net_dev.send_raw(eth_frame, arp_request.as_u8_buffer());
        net_dev.add_write(write);
    }

    WaitForEvent::new(NetEvent::ArpResponse(target_ip), waker_registry).await;
    LOGGER.log(format_args!("Resolving IP {} to MAC", target_ip));

    net_dev_lock.read().known_arp.get(&target_ip).cloned()
}

async fn get_next_hop(
    destination: Ipv4Address,
    net_dev_lock: Arc<RwLock<NetDevice>>,
    waker_registry: WakerRegistry<NetEvent>,
) -> Option<HardwareAddress> {
    let local_ip = get_local_ip(net_dev_lock.clone(), waker_registry.clone()).await?;
    let net_mask = net_dev_lock.read().dhcp_state.subnet_mask;
    let local_masked = local_ip & net_mask;
    let dest_masked = destination & net_mask;
    if local_masked == dest_masked {
        // If the destination is on the same subnet, we can use ARP to resolve it
        resolve_ip_to_mac(destination, net_dev_lock, waker_registry).await
    } else {
        // If the destination is not on the same subnet, we need to use a gateway
        let gateway = net_dev_lock.read().dhcp_state.gateway_ip;
        resolve_ip_to_mac(gateway, net_dev_lock, waker_registry).await
    }
}

async fn send_udp_packet(
    destination: Ipv4Address,
    source_port: u16,
    dest_port: u16,
    payload: Vec<u8>,
    net_dev_lock: Arc<RwLock<NetDevice>>,
    waker_registry: WakerRegistry<NetEvent>,
) -> Result<(), ()> {
    let local_ip = get_local_ip(net_dev_lock.clone(), waker_registry.clone())
        .await
        .ok_or(())?;
    let next_hop = match get_next_hop(destination, net_dev_lock.clone(), waker_registry).await {
        Some(mac) => mac,
        None => {
            LOGGER.log(format_args!("No route to {}", destination));
            return Err(());
        }
    };

    let eth_header = EthernetFrameHeader::new_ipv4(net_dev_lock.read().mac, next_hop);

    let udp_packet = create_datagram(local_ip, source_port, destination, dest_port, &payload);

    {
        let mut net_dev = net_dev_lock.write();
        let write = net_dev.send_raw(eth_header, &udp_packet);
        net_dev.add_write(write);
    }
    Ok(())
}
