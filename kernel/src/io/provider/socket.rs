//! Sockets require more functionality than strict file IO, so it's easier to
//! create a special-case provider for handling them.
//! The Socket provider allows tasks to send and receive data to/from a remote
//! client, listen to incoming data on a local port, and accept incoming
//! connections from remote clients. It also allows special functionality like
//! broadcasting and multicasting.

use idos_api::io::{error::IOError, AsyncOp};

use crate::{
    io::{async_io::AsyncOpID, handle::Handle},
    net::{
        ip::IPV4Address,
        socket::{bind_socket, create_socket, SocketHandle, SocketPort, SocketProtocol},
    },
};

use super::{IOProvider, IOResult, UnmappedAsyncOp};

#[repr(C, packed)]
pub struct SocketBindingRequest {
    local_ip: [u8; 4],
    local_port: u32,
    remote_ip: [u8; 4],
    remote_port: u32,
}

pub struct SocketIOProvider {
    socket_handle: SocketHandle,
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
        Self { socket_handle }
    }
}

impl IOProvider for SocketIOProvider {
    fn enqueue_op(&self, provider_index: u32, op: &AsyncOp, wake_set: Option<Handle>) -> AsyncOpID {
        unimplemented!()
    }

    fn get_active_op(&self) -> Option<(AsyncOpID, UnmappedAsyncOp)> {
        unimplemented!()
    }

    fn take_active_op(&self) -> Option<(AsyncOpID, UnmappedAsyncOp)> {
        unimplemented!()
    }

    fn pop_queued_op(&self) {
        unimplemented!()
    }

    /// Opening a socket binds it to a local or remote port
    /// The format of the IP addresses in the struct attached to the Op will
    /// determine what kind of port is opened.
    fn open(&self, _provider_index: u32, _id: AsyncOpID, op: UnmappedAsyncOp) -> Option<IOResult> {
        let binding_ptr = op.args[0] as *const SocketBindingRequest;
        let binding_len = op.args[1] as usize;
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

    fn read(&self, _provider_index: u32, _id: AsyncOpID, _op: UnmappedAsyncOp) -> Option<IOResult> {
        panic!("Not implemented");
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
