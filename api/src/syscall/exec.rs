use crate::ipc::Message;

pub fn terminate(code: u32) -> ! {
    super::syscall(0, code, 0, 0);
    unreachable!();
}

pub fn read_message_blocking(timeout: Option<u32>) -> Option<(u32, Message)> {
    let message = Message(0, 0, 0, 0);

    let encode_timeout = match timeout {
        Some(value) => value,
        None => 0xffffffff,
    };

    let sender = super::syscall(4, &message as *const Message as u32, encode_timeout, 0);
    if sender == 0 {
        return None;
    }
    Some((sender, message))
}

pub fn send_message(send_to: u32, message: Message, expiration: u32) {
    super::syscall(3, send_to, &message as *const Message as u32, expiration);
}

pub fn yield_coop() {
    super::syscall(6, 0, 0, 0);
}
