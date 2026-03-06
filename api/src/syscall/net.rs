use crate::io::Handle;

pub fn create_tcp_handle() -> Handle {
    Handle::new(super::syscall(0x26, 0, 0, 0))
}

pub fn create_udp_handle() -> Handle {
    Handle::new(super::syscall(0x25, 0, 0, 0))
}
