use super::{syscall, exec::yield_coop};

// TODO: Return a Task-specific id instead of a universal identifier
pub fn create_socket(protocol: u32) -> u32 {
    syscall(0x40, protocol, 0, 0)
}

pub fn bind_socket(socket: u32, local_ip: [u8; 4], local_port: u16, remote_ip: [u8; 4], remote_port: u16) {
    let local_binding: [u8; 6] = [
        local_ip[0],
        local_ip[1],
        local_ip[2],
        local_ip[3],
        local_port as u8,
        (local_port >> 8) as u8,
    ];
    let remote_binding: [u8; 6] = [
        remote_ip[0],
        remote_ip[1],
        remote_ip[2],
        remote_ip[3],
        remote_port as u8,
        (remote_port >> 8) as u8,
    ];

    syscall(0x41, socket, local_binding.as_ptr() as u32, remote_binding.as_ptr() as u32);
}

pub fn socket_accept(socket: u32) -> Option<u32> {
    loop {
        let accept = syscall(0x44, socket, 0, 0);
        if accept == 0 {
            yield_coop();
        } else {
            return Some(accept);
        }
    }
}

pub fn socket_read(socket: u32, buffer: &mut [u8]) -> Option<usize> {
    let len = syscall(0x42, socket, buffer.as_ptr() as u32, buffer.len() as u32) as usize;
    if len == 0 {
        None
    } else {
        Some(len)
    }
}
