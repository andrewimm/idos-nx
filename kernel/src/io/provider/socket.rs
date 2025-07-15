//! Sockets require more functionality than strict file IO, so it's easier to
//! create a special-case provider for handling them.
//! The Socket provider allows tasks to send and receive data to/from a remote
//! client, listen to incoming data on a local port, and accept incoming
//! connections from remote clients. It also allows special functionality like
//! broadcasting and multicasting.

use core::sync::atomic::Ordering;

use alloc::collections::BTreeMap;
use idos_api::io::{error::IOError, AsyncOp};
use spin::RwLock;

use crate::{
    io::{async_io::AsyncOpID, handle::Handle},
    net::{
        protocol::ipv4::Ipv4Address,
        socket::{socket_io_bind, socket_io_read, SocketId, SocketProtocol},
    },
    task::switching::get_current_id,
};

use super::{IOProvider, IOResult, OpIdGenerator, UnmappedAsyncOp};

pub struct SocketIOProvider {
    protocol: SocketProtocol,
    /// Currently bound socket ID. Newly created IO providers will not be bound
    /// until an open operation is completed.
    socket_id: RwLock<Option<u32>>,

    id_gen: OpIdGenerator,
    pending_ops: RwLock<BTreeMap<AsyncOpID, UnmappedAsyncOp>>,
}

impl SocketIOProvider {
    pub fn create_tcp() -> Self {
        Self::create_for_protocol(SocketProtocol::Tcp)
    }

    pub fn create_udp() -> Self {
        Self::create_for_protocol(SocketProtocol::Udp)
    }

    pub fn create_for_protocol(protocol: SocketProtocol) -> Self {
        Self {
            protocol,
            socket_id: RwLock::new(None),

            id_gen: OpIdGenerator::new(),
            pending_ops: RwLock::new(BTreeMap::new()),
        }
    }
}

impl IOProvider for SocketIOProvider {
    fn add_op(&self, provider_index: u32, op: &AsyncOp, wake_set: Option<Handle>) -> AsyncOpID {
        let id = self.id_gen.next_id();
        let unmapped =
            UnmappedAsyncOp::from_op(op, wake_set.map(|handle| (get_current_id(), handle)));
        self.pending_ops.write().insert(id, unmapped);

        match self.run_op(provider_index, id) {
            Some(result) => {
                self.remove_op(id);
                let return_value = self.transform_result(op.op_code, result);
                op.return_value.store(return_value, Ordering::SeqCst);
                op.signal.store(1, Ordering::SeqCst);
            }
            None => (),
        }
        id
    }

    fn bind_to(&self, instance: u32) {
        self.socket_id.write().replace(instance);
    }

    fn get_op(&self, id: AsyncOpID) -> Option<UnmappedAsyncOp> {
        self.pending_ops.read().get(&id).cloned()
    }

    fn remove_op(&self, id: AsyncOpID) -> Option<UnmappedAsyncOp> {
        self.pending_ops.write().remove(&id)
    }

    /// Opening a socket binds it to a local or remote port
    /// The format of the IP addresses in the struct attached to the Op will
    /// determine what kind of port is opened.
    fn open(&self, provider_index: u32, id: AsyncOpID, op: UnmappedAsyncOp) -> Option<IOResult> {
        if self.socket_id.read().is_some() {
            return Some(Err(IOError::AlreadyOpen));
        }
        let binding_addr: Ipv4Address = Ipv4Address([
            op.args[0] as u8,
            (op.args[0] >> 8) as u8,
            (op.args[0] >> 16) as u8,
            (op.args[0] >> 24) as u8,
        ]);
        let binding_port = op.args[1] as u16;
        let callback = (get_current_id(), provider_index, id);
        socket_io_bind(self.protocol, binding_addr, binding_port, callback)
    }

    fn read(&self, provider_index: u32, id: AsyncOpID, op: UnmappedAsyncOp) -> Option<IOResult> {
        let socket_id = *self.socket_id.read();
        if let Some(socket_id) = socket_id {
            let buffer_start = op.args[0] as usize;
            let buffer_len = op.args[1] as usize;
            let buffer =
                unsafe { core::slice::from_raw_parts_mut(buffer_start as *mut u8, buffer_len) };
            let callback = (get_current_id(), provider_index, id);
            socket_io_read(SocketId::new(socket_id), buffer, callback)
        } else {
            Some(Err(IOError::FileHandleInvalid))
        }
    }

    fn write(
        &self,
        _provider_index: u32,
        _id: AsyncOpID,
        _op: UnmappedAsyncOp,
    ) -> Option<IOResult> {
        panic!("Not implemented");
    }

    fn extended_op(
        &self,
        _provider_index: u32,
        _id: AsyncOpID,
        _op: UnmappedAsyncOp,
    ) -> Option<IOResult> {
        panic!("Not implemented");
    }
}
