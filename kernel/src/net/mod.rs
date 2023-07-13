//! The net stack handles all IP traffic for user programs, as well as any
//! services like ARP, DHCP, or DNS.
//!
//! Network device drivers are registered, and can be used by the
//! Eventually, it will be possible to use multiple devices in parallel. For
//! now the "active" device is the one that's used for everything.
//!
//! Core to the net stack is a Task that constantly reads packets from the
//! active device. Depending on the type of packet, it is routed to one of the
//! different subsystems and handled accordingly. For example, ARP packets will
//! go to update the ARP cache, IP packets will go to the socket that is
//! talking to the sender, etc. Outgoing packets don't usually go through this
//! task, but they may block on information that must be received first.

pub mod arp;
pub mod dhcp;
pub mod ethernet;
pub mod ip;
pub mod packet;
pub mod udp;

use core::ops::Deref;
use crate::collections::SlotList;
use spin::RwLock;

#[repr(transparent)]
pub struct NetID(u32);

#[derive(Clone)]
pub struct NetDevice {
    pub mac: [u8; 6],
}

impl NetDevice {
    pub fn new(mac: [u8; 6]) -> Self {
        Self {
            mac,
        }
    }
}

static NET_DEVICES: RwLock<SlotList<NetDevice>> = RwLock::new(SlotList::new());

static ACTIVE_DEVICE: RwLock<Option<NetDevice>> = RwLock::new(None);

pub fn register_network_interface(mac: [u8; 6]) -> NetID {
    let device = NetDevice::new(mac);
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

