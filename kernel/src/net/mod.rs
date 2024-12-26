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
pub mod checksum;
pub mod dhcp;
pub mod error;
pub mod ethernet;
pub mod ip;
pub mod packet;
pub mod socket;
pub mod tcp;
pub mod udp;

use self::{
    dhcp::start_dhcp_transaction, ethernet::EthernetFrame, ip::IPV4Address, packet::PacketHeader,
};
use crate::collections::SlotList;
use crate::io::handle::Handle;
use crate::task::actions::handle::{
    create_file_handle, create_pipe_handles, handle_op_close, handle_op_open, handle_op_read,
    handle_op_write, transfer_handle,
};
use crate::task::actions::io::{close_file, open_path, open_pipe, read_file, write_file};
use crate::task::actions::lifecycle::{create_kernel_task, wait_for_io};
use crate::task::files::FileHandle;
use crate::task::id::TaskID;
use crate::task::switching::{get_current_id, get_task};
use alloc::{string::String, sync::Arc, vec::Vec};
use core::ops::Deref;
use core::sync::atomic::{AtomicU32, Ordering};
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
        let dev = create_file_handle();
        handle_op_open(dev, &self.device_name)
            .wait_for_result()
            .unwrap();
        handle_op_write(dev, raw).wait_for_result().unwrap();
        handle_op_close(dev).wait_for_result().unwrap();
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
where
    F: Fn(&NetDevice) -> T,
{
    let device = ACTIVE_DEVICE.read();
    match device.deref() {
        Some(dev) => Ok(f(dev)),
        None => Err(()),
    }
}

pub fn get_net_device_by_mac(mac: [u8; 6]) -> Option<Arc<NetDevice>> {
    NET_DEVICES
        .read()
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

    let response_writer = Handle::new(0);
    handle_op_write(response_writer, &[1]);

    let mut read_buffer = Vec::with_capacity(1024);
    for _ in 0..1024 {
        read_buffer.push(0);
    }

    let eth_dev = create_file_handle();
    handle_op_open(eth_dev, "DEV:\\ETH").wait_for_completion();

    loop {
        //PACKETS_RECEIVED.store(0, Ordering::SeqCst);
        let len = handle_op_read(eth_dev, &mut read_buffer, 0)
            .wait_for_result()
            .unwrap() as usize;
        if len > 0 {
            match EthernetFrame::from_buffer(&read_buffer)
                .map(|frame| (frame.get_ethertype(), frame.src_mac))
            {
                Some((self::ethernet::ETHERTYPE_ARP, _)) => {
                    self::arp::handle_arp_announcement(&read_buffer[EthernetFrame::get_size()..]);
                }
                Some((self::ethernet::ETHERTYPE_IP, src_mac)) => {
                    self::socket::receive_ip_packet(
                        src_mac,
                        &read_buffer[EthernetFrame::get_size()..],
                    );
                }
                _ => (),
            }
        }
    }
}

pub fn start_net_stack() {
    let (response_reader, response_writer) = create_pipe_handles();

    let driver_task = create_kernel_task(net_stack_task, Some("NET"));
    transfer_handle(response_writer, driver_task).unwrap();
    // wait for a response from the driver indicating initialization
    handle_op_read(response_reader, &mut [0u8], 0).wait_for_completion();
}
