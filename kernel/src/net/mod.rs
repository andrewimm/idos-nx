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
pub mod resident;
pub mod socket;
pub mod tcp;
pub mod udp;

use self::ethernet::HardwareAddress;
use self::{dhcp::start_dhcp_transaction, ip::IPV4Address};
use crate::collections::SlotList;
use crate::task::actions::handle::{
    create_file_handle, create_kernel_task, create_pipe_handles, handle_op_close, handle_op_open,
    handle_op_read, handle_op_write, transfer_handle,
};
use crate::task::actions::lifecycle::wait_for_io;
use alloc::{string::String, sync::Arc};
use core::ops::Deref;
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
    pub mac: HardwareAddress,
    pub device_name: String,
    pub ip: RwLock<Option<self::ip::IPV4Address>>,
}

impl NetDevice {
    pub fn new(mac: HardwareAddress, device_name: String) -> Self {
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
    let device = Arc::new(NetDevice::new(
        HardwareAddress(mac),
        String::from(device_name),
    ));
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

pub fn get_net_device_by_mac(mac: HardwareAddress) -> Option<Arc<NetDevice>> {
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
    start_dhcp_transaction(mac);
    wait_for_io(timeout);

    match with_active_device(|netdev| *netdev.ip.read()) {
        Ok(Some(ip)) => Some(ip),
        _ => None,
    }
}

pub fn start_net_stack() {
    let (response_reader, response_writer) = create_pipe_handles();

    //let driver_task = create_kernel_task(net_stack_task, Some("NET"));
    let (_, driver_task) = create_kernel_task(resident::net_stack_resident, Some("NETR"));
    transfer_handle(response_writer, driver_task).unwrap();
    // wait for a response from the driver indicating initialization
    handle_op_read(response_reader, &mut [0u8], 0).wait_for_completion();
}
