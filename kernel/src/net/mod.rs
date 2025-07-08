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
//pub mod executor;
pub mod hardware;
pub mod ip;
//pub mod netdevice;
pub mod packet;
pub mod protocol;
pub mod resident;
pub mod socket;
pub mod tcp;
pub mod udp;

use self::ethernet::HardwareAddress;
use crate::task::actions::{
    handle::{create_file_handle, create_kernel_task, create_pipe_handles, transfer_handle},
    io::{close_sync, open_sync, read_sync, write_sync},
};
use alloc::{string::String, sync::Arc};
use core::ops::Deref;
use spin::RwLock;

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
        open_sync(dev, &self.device_name).unwrap();
        write_sync(dev, raw, 0).unwrap();
        close_sync(dev).unwrap();
    }
}

static ACTIVE_DEVICE: RwLock<Option<Arc<NetDevice>>> = RwLock::new(None);

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

pub fn start_net_stack() {
    let (response_reader, response_writer) = create_pipe_handles();

    let (_, driver_task) = create_kernel_task(resident::net_stack_resident, Some("NETR"));
    transfer_handle(response_writer, driver_task).unwrap();
    // wait for a response from the driver indicating initialization
    let _ = read_sync(response_reader, &mut [0u8], 0);
}
