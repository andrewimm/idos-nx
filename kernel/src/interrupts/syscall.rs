use core::arch::global_asm;

use crate::task::{actions::{self, send_message, read_message_blocking}, id::TaskID, messaging::Message};

use super::stack::StackFrame;

global_asm!(r#"
.global syscall_handler

syscall_handler:
    push eax
    push ecx
    push edx
    push ebx
    push ebp
    push esi
    push edi
    mov ebx, esp
    push ebx
    add ebx, 7 * 4
    push ebx

    call _syscall_inner

    add esp, 8
    pop edi
    pop esi
    pop ebp
    pop ebx
    pop edx
    pop ecx
    pop eax

    iretd
"#);

#[repr(C, packed)]
pub struct SavedRegisters {
    edi: u32,
    esi: u32,
    ebp: u32,
    ebx: u32,
    edx: u32,
    ecx: u32,
    eax: u32,
}

impl core::fmt::Debug for SavedRegisters {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let eax = self.eax;
        let ebx = self.ebx;
        let ecx = self.ecx;
        let edx = self.edx;
        let ebp = self.ebp;
        let esi = self.esi;
        let edi = self.edi;
        f.debug_struct("Saved Registers")
            .field("eax", &format_args!("{:#010X}", eax))
            .field("ebx", &format_args!("{:#010X}", ebx))
            .field("ecx", &format_args!("{:#010X}", ecx))
            .field("edx", &format_args!("{:#010X}", edx))
            .field("ebp", &format_args!("{:#010X}", ebp))
            .field("esi", &format_args!("{:#010X}", esi))
            .field("edi", &format_args!("{:#010X}", edi))
            .finish()
    }
}

#[no_mangle]
pub extern "C" fn _syscall_inner(_frame: &StackFrame, registers: &mut SavedRegisters) {
    crate::kprint!("REG: {:?}\n", registers);
    let eax = registers.eax;
    match eax {
        // task lifecycle and interop
        0x00 => { // exit
            let code = registers.ebx;
            actions::lifecycle::terminate(code);
        },
        0x01 => { // create task
        },
        0x02 => { // wait (single task or all)
        },
        0x03 => { // send message
            let send_to = TaskID::new(registers.ebx);
            let message_ptr = registers.ecx as *const Message;
            let message = unsafe { &*message_ptr };
            let expiration = registers.edx;
            send_message(send_to, *message, expiration)
        },
        0x04 => { // receive message
            let message_ptr = registers.ebx as *mut Message;
            let timeout = match registers.ecx {
                0xffffffff => None,
                time => Some(time),
            };
            match read_message_blocking(timeout) {
                (Some(packet), _) => {
                    let (from, message) = packet.open();
                    unsafe { core::ptr::write_volatile(message_ptr, message) };
                    registers.eax = from.into();
                },
                (None, _) => {
                    registers.eax = 0;
                },
            };
        },
        0x05 => { // sleep
            let duration = registers.ebx;
            actions::sleep(duration);
        },
        0x06 => { // yield coop
            actions::yield_coop();
        },
        0x07 => { // mmap
        },
        0x08 => { //
        },
        0x09 => { //
        },
        0x0a => {
        },

        // io
        0x10 => { // open path
        },
        0x11 => { // close handle
        },
        0x12 => { // read
            let handle = crate::task::files::FileHandle::new(registers.ebx as usize);
            let dest_ptr = registers.ecx as *mut u8;
            let length = registers.edx as usize;
            let buffer = unsafe { core::slice::from_raw_parts_mut(dest_ptr, length) };
            match actions::io::read_file(handle, buffer) {
                Ok(written) => {
                    registers.eax = written as u32;
                },
                Err(_) => {
                    registers.eax = 0;
                },
            }
        },
        0x13 => { // write
            let handle = crate::task::files::FileHandle::new(registers.ebx as usize);
            let src_ptr = registers.ecx as *const u8;
            let length = registers.edx as usize;
            let buffer = unsafe { core::slice::from_raw_parts(src_ptr, length) };
            match actions::io::write_file(handle, buffer) {
                Ok(written) => {
                    registers.eax = written as u32;
                },
                Err(_) => {
                    registers.eax = 0;
                },
            }
        },

        // drivers
        0x30 => { // register fs
        },
        0x31 => { // register device
            
        },

        // net
        0x40 => { // create socket
            // TODO: use a task-specific handle instead of a universal id
            let protocol = match registers.ebx {
                1 => crate::net::socket::SocketProtocol::TCP,
                _ => crate::net::socket::SocketProtocol::UDP,
            };
            let id = crate::net::socket::create_socket(protocol);
            registers.eax = *id;
        },
        0x41 => { // bind socket
            let socket_id = crate::net::socket::SocketHandle(registers.ebx);
            let local_binding_ptr = registers.ecx as *const u8;
            let remote_binding_ptr = registers.edx as *const u8;
            let local_ip = crate::net::ip::IPV4Address(
                unsafe { core::slice::from_raw_parts(local_binding_ptr, 4).try_into().unwrap() }
            );
            let local_port_raw = unsafe { 
                (*local_binding_ptr.offset(4) as u16) |
                ((*local_binding_ptr.offset(5) as u16) << 8)
            };
            let local_port = crate::net::socket::SocketPort::new(local_port_raw);
            let remote_ip = crate::net::ip::IPV4Address(
                unsafe { core::slice::from_raw_parts(remote_binding_ptr, 4).try_into().unwrap() }
            );
            let remote_port_raw = unsafe {
                (*remote_binding_ptr.offset(4) as u16) |
                ((*remote_binding_ptr.offset(5) as u16) << 8)
            };
            let remote_port = crate::net::socket::SocketPort::new(remote_port_raw);
            match crate::net::socket::bind_socket(socket_id, local_ip, local_port, remote_ip, remote_port) {
                Ok(_) => registers.eax = 0,
                Err(_) => registers.eax = 1,
            }
        },
        0x42 => { // socket read
            let socket_id = crate::net::socket::SocketHandle(registers.ebx);
            let buffer_ptr = registers.ecx as *mut u8;
            let buffer_len = registers.edx as usize;
            let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
            match crate::net::socket::socket_read(socket_id, buffer) {
                Some(len) => registers.eax = len as u32,
                None => registers.eax = 0,
            }
        },
        0x43 => { // socket write
        },
        0x44 => { // socket accept
            let socket_id = crate::net::socket::SocketHandle(registers.ebx);
            match crate::net::socket::socket_accept(socket_id) {
                Some(id) => registers.eax = *id,
                None => registers.eax = 0,
            }
        },

        0xffff => {
            crate::kprint!("\n\nSyscall: DEBUG\n");
            registers.eax = 0;
        },
        _ => panic!("Unknown Syscall!"),
    }
}
