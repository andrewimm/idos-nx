use core::arch::global_asm;

use idos_api::io::AsyncOp;

use crate::{
    io::handle::Handle,
    memory::address::{PhysicalAddress, VirtualAddress},
    task::{
        actions::{self, memory::map_memory, send_message},
        id::TaskID,
        memory::MemoryBacking,
        messaging::Message,
        switching::get_task,
    },
};

use super::stack::StackFrame;

global_asm!(
    r#"
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
"#
);

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
    crate::kprint!("SYSCALL REG: {:?}\n", registers);
    let eax = registers.eax;
    match eax {
        // task lifecycle and interop
        0x00 => {
            // exit
            let code = registers.ebx;
            actions::lifecycle::terminate(code);
        }
        0x01 => {
            // yield coop
            actions::yield_coop();
        }
        0x02 => {
            // sleep
            let duration = registers.ebx;
            actions::sleep(duration);
        }
        0x03 => {
            // get current task id
            let current = crate::task::switching::get_current_id();
            registers.eax = current.into();
        }
        0x04 => {
            // get parent task id
            let current = crate::task::switching::get_current_id();
            let parent = crate::task::switching::get_task(current)
                .unwrap()
                .read()
                .parent_id;
            registers.eax = parent.into();
        }
        0x05 => {
            // add args
            let task_id = TaskID::new(registers.ebx);
            match get_task(task_id) {
                Some(task) => {
                    registers.eax = 1;
                }
                None => {
                    registers.eax = 0xffff_ffff;
                }
            }
        }
        0x06 => {
            // load executable
            let task_id = TaskID::new(registers.ebx);
            let path_ptr = registers.ecx as *const u8;
            let path_len = registers.edx as usize;
            let path = unsafe {
                core::str::from_utf8_unchecked(core::slice::from_raw_parts(path_ptr, path_len))
            };
            match crate::loader::load_executable(task_id, path) {
                Ok(_) => registers.eax = 1,
                Err(_) => registers.eax = 0xffff_ffff,
            }
        }

        // IO Actions
        0x10 => {
            // submit async io op
            let handle = Handle::new(registers.ebx as usize);
            let op_ptr = registers.ecx as *const AsyncOp;
            let op = unsafe { &*op_ptr };
            let wake_set = match registers.edx {
                0xffff_ffff => None,
                edx => Some(Handle::new(registers.edx as usize)),
            };
            match actions::io::append_io_op(handle, op, wake_set) {
                Ok(_) => registers.eax = 1,
                Err(e) => registers.eax = 0x8000_0000,
            }
        }
        0x11 => {
            // send message
            let send_to = TaskID::new(registers.ebx);
            let message_ptr = registers.ecx as *const Message;
            let message = unsafe { &*message_ptr };
            let expiration = registers.edx;
            send_message(send_to, *message, expiration);
        }
        0x12 => {
            // driver io complete
            unimplemented!()
        }
        0x13 => {
            // futex wait
            let address = VirtualAddress::new(registers.ebx);
            let value = registers.ecx;
            let timeout = match registers.edx {
                0xffff_ffff => None,
                edx => Some(edx),
            };
            crate::sync::futex::futex_wait(address, value, timeout)
        }
        0x14 => {
            // futex wake
            let address = VirtualAddress::new(registers.ebx);
            let count = registers.ecx;
            crate::sync::futex::futex_wake(address, count)
        }
        0x15 => {
            // create wake set
            let handle = actions::sync::create_wake_set();
            registers.eax = *handle as u32;
        }
        0x16 => {
            // block on wake set
            let handle = Handle::new(registers.ebx as usize);
            let timeout = match registers.ecx {
                0xffff_ffff => None,
                edx => Some(edx),
            };
            actions::sync::block_on_wake_set(handle, timeout);
        }

        // handle actions
        0x20 => {
            // create task
            let (handle, task_id) = actions::handle::create_task();
            registers.eax = *handle as u32;
            registers.ebx = task_id.into();
        }
        0x21 => {
            // create message queue handle
            let handle = actions::handle::open_message_queue();
            registers.eax = *handle as u32;
        }
        0x22 => {
            // create irq handle
            let irq = registers.ebx;
            let handle = actions::handle::open_interrupt_handle(irq as u8);
            registers.eax = *handle as u32;
        }
        0x23 => {
            // create file handle
            let handle = actions::handle::create_file_handle();
            registers.eax = *handle as u32;
        }
        0x24 => {
            // create pipe handles
            let (read_handle, write_handle) = actions::handle::create_pipe_handles();
            registers.eax = *read_handle as u32;
            registers.ebx = *write_handle as u32;
        }
        0x25 => {
            // create udp socket handle
            unimplemented!()
        }
        0x26 => {
            // create tcp socket handle
            unimplemented!()
        }

        0x2a => {
            // transfer handle
            let handle = Handle::new(registers.ebx as usize);
            let task_id = TaskID::new(registers.ecx);
            let result = actions::handle::transfer_handle(handle, task_id);
            registers.eax = match result {
                Some(handle) => *handle as u32,
                None => 0xffff_ffff,
            }
        }
        0x2b => {
            // dup handle
            let handle = Handle::new(registers.ebx as usize);
            let result = actions::handle::dup_handle(handle);
            registers.eax = match result {
                Some(handle) => *handle as u32,
                None => 0xffff_ffff,
            }
        }

        // memory actions
        0x30 => {
            // map memory
            let address = match registers.ebx {
                0xffff_ffff => None,
                ebx => Some(VirtualAddress::new(ebx)),
            };
            let size = registers.ecx;
            let backing = match registers.edx {
                0xffff_ffff => MemoryBacking::Anonymous,
                address => MemoryBacking::Direct(PhysicalAddress::new(address)),
            };
            match map_memory(address, size, backing) {
                Ok(vaddr) => {
                    registers.eax = vaddr.into();
                }
                Err(e) => {
                    // TODO: we need error codes
                    registers.eax = 0xffff_ffff;
                }
            }
        }

        0xffff => {
            crate::kprint!("\n\nSyscall: DEBUG\n");
            registers.eax = 0;
        }
        _ => panic!("Unknown Syscall!"),
    }
}
