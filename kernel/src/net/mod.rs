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

