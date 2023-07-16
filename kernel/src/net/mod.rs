//! The net stack handles all IP traffic for user programs, as well as any
//! services like ARP, DHCP, or DNS.
//!
//! Core to the net stack is a Task that constantly reads packets from the
//! active device. Depending on the type of packet, it is routed to one of the
//! different subsystems and handled accordingly. For example, ARP packets will
//! go to update the ARP cache, IP packets will go to the socket that is
//! talking to the sender, etc. Outgoing packets don't usually go through this
//! task, but they may block on information that must be received first.
//!
//! When a new network device is registered, the net stack will attempt to
//! assign a local IP address to that device via DHCP. The first device with an
//! assigned IP will become the "default" device, and any sockets will bind to
//! that unless otherwise specified.
//! A socket can be created and opened, but cannot read/write until it is bound
//! to an IP address and port. There are two ways to bind a socket:
//!  - A listener binds to a specific local port, and can read all incoming
//!    packets. It will not be associated with a remote endpoint.
//!  - A socket bound to a remote location will automatically be assigned a
//!    port. Traffic from the local host will appear to come from this port,
//!    and any traffic from the remote end will be addressed to that port.
//!
//! The net task reads packets from all network devices. When a packet arrives,
//! it inspects the packet, unwrapping headers, and determines where to send
//! it.
//!

pub mod arp;
pub mod dhcp;
pub mod error;
pub mod ethernet;
pub mod ip;
pub mod packet;
pub mod socket;
pub mod udp;

use core::{ops::Deref, sync::atomic::{AtomicU32, Ordering}};
use crate::{collections::SlotList, task::{actions::{yield_coop, io::{open_path, read_file, open_pipe, transfer_handle, write_file, close_file}, lifecycle::{create_kernel_task, wait_for_io}}, files::FileHandle, switching::{get_task, get_current_id}, id::TaskID}, net::ethernet::EthernetFrame};
use alloc::{vec::Vec, string::String, sync::Arc};
use self::{packet::PacketHeader, ip::IPV4Address, dhcp::start_dhcp_transaction};
use spin::RwLock;

#[repr(transparent)]
pub struct NetID(u32);

impl core::ops::Deref for NetID {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct NetDevice {
    pub mac: [u8; 6],
    pub device_name: String,
    pub ip: RwLock<Option<self::ip::IPV4Address>>,
}

impl NetDevice {
    pub fn new(mac: [u8; 6], device_name: String) -> Self {
        Self {
            mac,
            device_name,
            ip: RwLock::new(None),
        }
    }

    pub fn send_raw(&self, raw: &[u8]) {
        let dev = open_path(&self.device_name).unwrap();
        write_file(dev, raw).unwrap();
        close_file(dev).unwrap();
    }
}

static NET_DEVICES: RwLock<SlotList<Arc<NetDevice>>> = RwLock::new(SlotList::new());

static ACTIVE_DEVICE: RwLock<Option<Arc<NetDevice>>> = RwLock::new(None);

pub fn register_network_interface(mac: [u8; 6], device_name: &str) -> NetID {
    let device = Arc::new(NetDevice::new(mac, String::from(device_name)));
    let index = NET_DEVICES.write().insert(device.clone()) as u32;

    let mut active = ACTIVE_DEVICE.write();
    if active.is_none() {
        active.replace(device);
    }

    NetID(index)
}

pub fn with_active_device<F, T>(f: F) -> Result<T, ()>
    where F: Fn(&NetDevice) -> T {

    let device = ACTIVE_DEVICE.read();
    match device.deref() {
        Some(dev) => Ok(f(dev)),
        None => Err(()),
    }
}

pub fn get_net_device_by_mac(mac: [u8; 6]) -> Option<Arc<NetDevice>> {
    NET_DEVICES.read()
        .iter()
        .find(|dev| dev.mac == mac)
        .cloned()
}

pub fn get_active_device_ip(timeout: Option<u32>) -> Option<IPV4Address> {
    let (mac, stored_ip) = match with_active_device(|netdev| (netdev.mac, *netdev.ip.read())) {
        Ok(pair) => pair,
        Err(_) => return None,
    };
    match stored_ip {
        Some(stored) => return Some(stored),
        _ => (),
    }
    let current_id = get_current_id();
    start_dhcp_transaction(current_id, mac);
    wait_for_io(timeout);

    match with_active_device(|netdev| *netdev.ip.read()) {
        Ok(Some(ip)) => Some(ip),
        _ => None,
    }
}

static NET_TASK_ID: AtomicU32 = AtomicU32::new(0);
static PACKETS_RECEIVED: AtomicU32 = AtomicU32::new(0);

pub fn notify_net_device_ready(_id: u32) {
    PACKETS_RECEIVED.fetch_add(1, Ordering::SeqCst);
    let task_id = TaskID::new(NET_TASK_ID.load(Ordering::SeqCst));
    if let Some(lock) = get_task(task_id) {
        lock.write().io_complete();
    }
}

fn net_stack_task() -> ! {
    let current_id = get_current_id();
    NET_TASK_ID.store(current_id.into(), Ordering::SeqCst);

    let response_writer = FileHandle::new(0);
    write_file(response_writer, &[1]).unwrap();

    let mut read_buffer = Vec::with_capacity(1024);
    for i in 0..1024 {
        read_buffer.push(0);
    }

    let eth_dev = open_path("DEV:\\ETH").unwrap();

    loop {
        PACKETS_RECEIVED.store(0, Ordering::SeqCst);
        let len = read_file(eth_dev, &mut read_buffer).unwrap() as usize;
        if len > 0 {
            match EthernetFrame::from_buffer(&read_buffer).map(|frame| frame.get_ethertype()) {
                Some(self::ethernet::ETHERTYPE_ARP) => {
                    crate::kprintln!("ARP PACKET");
                    self::arp::handle_arp_announcement(&read_buffer[EthernetFrame::get_size()..]);
                },
                Some(self::ethernet::ETHERTYPE_IP) => {
                    crate::kprintln!("IP PACKET");
                    self::socket::receive_ip_packet(&read_buffer[EthernetFrame::get_size()..]);
                },
                _ => (),
            }
        }
        let received_while_processing = PACKETS_RECEIVED.swap(0, Ordering::SeqCst);
        if received_while_processing == 0 {
            wait_for_io(Some(1000));
        }
    }
}

pub fn start_net_stack() {
    let (response_reader, response_writer) = open_pipe().unwrap();

    let driver_task = create_kernel_task(net_stack_task, Some("NET"));
    transfer_handle(response_writer, driver_task).unwrap();
    // wait for a response from the driver indicating initialization
    read_file(response_reader, &mut [0u8]).unwrap();
}
