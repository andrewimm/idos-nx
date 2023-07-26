//! When packets arrive to an established connection, the received packet
//! payloads are queued up to be read from the socket handle

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use spin::RwLock;
use super::super::socket::SocketHandle;

static QUEUED_PACKETS: RwLock<BTreeMap<SocketHandle, VecDeque<Vec<u8>>>> = RwLock::new(BTreeMap::new());

pub fn get_latest_packet(handle: SocketHandle) -> Option<Vec<u8>> {
    let mut tree = QUEUED_PACKETS.write();
    let queue = tree.get_mut(&handle)?;
    queue.pop_front()
}

pub fn add_packet(handle: SocketHandle, packet: Vec<u8>) {
    let mut tree = QUEUED_PACKETS.write();
    match tree.get_mut(&handle) {
        Some(queue) => queue.push_back(packet),
        None => {
            let mut queue = VecDeque::with_capacity(1);
            queue.push_back(packet);
            tree.insert(handle, queue);
        },
    }
}
