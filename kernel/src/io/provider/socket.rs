//! Sockets require more functionality than strict file IO, so it's easier to
//! create a special-case provider for handling them.
//! The Socket provider allows tasks to send and receive data to/from a remote
//! client, listen to incoming data on a local port, and accept incoming
//! connections from remote clients. It also allows special functionality like
//! broadcasting and multicasting.

use idos_api::io::error::IOError;

use crate::{
    io::async_io::{AsyncOp, AsyncOpID, AsyncOpQueue},
    net::{
        ip::IPV4Address,
        socket::{bind_socket, create_socket, SocketHandle, SocketPort, SocketProtocol},
    },
};

use super::{IOProvider, IOResult};

#[repr(C, packed)]
pub struct SocketBindingRequest {
    local_ip: [u8; 4],
    local_port: u32,
    remote_ip: [u8; 4],
    remote_port: u32,
}

pub struct SocketIOProvider {
    socket_handle: SocketHandle,
    pending_ops: AsyncOpQueue,
}

impl SocketIOProvider {
    pub fn create_tcp() -> Self {
        Self::create_for_protocol(SocketProtocol::TCP)
    }

    pub fn create_udp() -> Self {
        Self::create_for_protocol(SocketProtocol::UDP)
    }

    pub fn create_for_protocol(protocol: SocketProtocol) -> Self {
        let socket_handle = create_socket(protocol);
        Self {
            socket_handle,
            pending_ops: AsyncOpQueue::new(),
        }
    }
}

impl IOProvider for SocketIOProvider {
    fn enqueue_op(&self, op: AsyncOp) -> (AsyncOpID, bool) {
        let id = self.pending_ops.push(op);
        let should_run = self.pending_ops.len() < 2;
        (id, should_run)
    }

    fn peek_op(&self) -> Option<(AsyncOpID, AsyncOp)> {
        self.pending_ops.peek()
    }

    fn remove_op(&self, id: AsyncOpID) -> Option<AsyncOp> {
        self.pending_ops.remove(id)
    }

    /// Opening a socket binds it to a local or remote port
    /// The format of the IP addresses in the struct attached to the Op will
    /// determine what kind of port is opened.
    fn open(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<IOResult> {
        let binding_ptr = op.arg0 as *const SocketBindingRequest;
        let binding_len = op.arg1 as usize;
        if binding_len != core::mem::size_of::<SocketBindingRequest>() {
            return Some(Err(IOError::InvalidArgument));
        }
        let binding_request = unsafe { &*binding_ptr };
        let local_ip = IPV4Address(binding_request.local_ip);
        let remote_ip = IPV4Address(binding_request.remote_ip);
        let local_port = SocketPort::new(binding_request.local_port as u16);
        let remote_port = SocketPort::new(binding_request.remote_port as u16);

        Some(
            bind_socket(
                self.socket_handle,
                local_ip,
                local_port,
                remote_ip,
                remote_port,
            )
            .map(|_| 1)
            .map_err(|_| IOError::OperationFailed),
        )
    }

    fn read(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<IOResult> {
        panic!("Not implemented");
    }

    fn write(&self, provider_index: u32, id: AsyncOpID, op: AsyncOp) -> Option<IOResult> {
        panic!("Not implemented");
    }
}
