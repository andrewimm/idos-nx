#![no_std]
#![no_main]
#![feature(lang_items)]

use core::arch::asm;
use core::panic::PanicInfo;

fn syscall(a: u32, b: u32, c: u32, d: u32) -> u32 {
    let result: u32;
    unsafe {
        asm!(
            "int 0x2b",
            inout("eax") a => result,
            in("ebx") b,
            in("ecx") c,
            in("edx") d,
        );
    }
    result
}

#[no_mangle]
extern "C" fn _start() {
    

    loop {
        let message_read = read_message_blocking(None);
        if let Some((sender, message)) = message_read {
            match handle_request(message) {
                Some(response) => send_message(sender, response, 0xffffffff),
                None => continue,
            }
        }
    }
    //syscall(0, 1, 0, 0);
}

fn handle_request(message: Message) -> Option<Message> {
    match message.0 {
        2 => { // OpenRaw
            Some((1, 0, 0))
        },
        3 => { // read
            let open_instance = message.1;
            let buffer_start = message.2 as *mut u8;
            let buffer_len = message.3 as usize;

            let buffer = unsafe {
                core::slice::from_raw_parts_mut(buffer_start, buffer_len)
            };
            let bytes = b"test";
            let len = buffer.len().min(bytes.len());

            buffer[..len].copy_from_slice(&bytes[..len]);

            Some((len as u32, 0, 0))
        },
        5 => { // close
            let handle = message.1 as u32;
            Some((0, 0, 0))
        },
        _ => {
            None
        },
    }.map(|(a, b, c)| Message(0x00524553, a, b, c))
}

struct Message(u32, u32, u32, u32);

fn read_message_blocking(timeout: Option<u32>) -> Option<(u32, Message)> {
    let mut message = Message(0, 0, 0, 0);

    let sender = syscall(4, &message as *const Message as u32, 0xffffffff, 0);
    if sender == 0 {
        return None;
    }
    Some((sender, message))
}

fn send_message(send_to: u32, message: Message, expiration: u32) {
    syscall(3, send_to, &message as *const Message as u32, expiration);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}
