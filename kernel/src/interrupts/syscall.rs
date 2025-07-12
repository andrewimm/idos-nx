use core::arch::global_asm;

use idos_api::{compat::VMRegisters, io::AsyncOp};

use crate::{
    io::handle::Handle,
    memory::address::{PhysicalAddress, VirtualAddress},
    task::{
        actions::{self, memory::map_memory, send_message},
        id::TaskID,
        map::get_task,
        memory::MemoryBacking,
        messaging::Message,
    },
};

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

    call _syscall_inner

    add esp, 4
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

#[derive(Clone)]
#[repr(C, packed)]
pub struct SavedRegisters {
    pub edi: u32,
    pub esi: u32,
    pub ebp: u32,
    pub ebx: u32,
    pub edx: u32,
    pub ecx: u32,
    pub eax: u32,
}

#[derive(Clone)]
#[repr(C, packed)]
pub struct FullSavedRegisters {
    pub edi: u32,
    pub esi: u32,
    pub ebp: u32,
    pub ebx: u32,
    pub edx: u32,
    pub ecx: u32,
    pub eax: u32,

    pub eip: u32,
    pub cs: u32,
    pub eflags: u32,
    pub esp: u32,
    pub ss: u32,
}

impl core::fmt::Debug for FullSavedRegisters {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let eax = self.eax;
        let ebx = self.ebx;
        let ecx = self.ecx;
        let edx = self.edx;
        let ebp = self.ebp;
        let esi = self.esi;
        let edi = self.edi;

        let eip = self.eip;
        let cs = self.cs;
        let eflags = self.eflags;
        let esp = self.esp;
        let ss = self.ss;
        f.debug_struct("Saved Registers")
            .field("eax", &format_args!("{:#010X}", eax))
            .field("ebx", &format_args!("{:#010X}", ebx))
            .field("ecx", &format_args!("{:#010X}", ecx))
            .field("edx", &format_args!("{:#010X}", edx))
            .field("ebp", &format_args!("{:#010X}", ebp))
            .field("esi", &format_args!("{:#010X}", esi))
            .field("edi", &format_args!("{:#010X}", edi))
            .field("eip", &format_args!("{:#010X}", eip))
            .field("cs", &format_args!("{:#010X}", cs))
            .field("eflags", &format_args!("{:#010X}", eflags))
            .field("esp", &format_args!("{:#010X}", esp))
            .field("ss", &format_args!("{:#010X}", ss))
            .finish()
    }
}

#[no_mangle]
pub extern "C" fn _syscall_inner(registers: &mut FullSavedRegisters) {
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
            let parent = get_task(current).unwrap().read().parent_id;
            registers.eax = parent.into();
        }
        0x05 => {
            // add args
            let task_id = TaskID::new(registers.ebx);
            match get_task(task_id) {
                Some(task) => {
                    // TODO: implement arg appends
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
        0x07 => {
            // enter 8086 VM mode
            // This syscall is more complex than the rest, since it will not
            // return. Instead it will switch to 8086 mode and begin executing
            // somewhere else. The first time that code triggers a GPF, it will
            // appear as though this syscall has returned.
            // In order for that to work, we need to save the registers now.
            // If a GPF is found to have started in the VM, we can restore the
            // registers and IRET, returning to the callsite in userspace.
            let regs_ptr = registers.ebx as *mut VMRegisters;
            crate::task::actions::vm::enter_vm86_mode(registers, regs_ptr);
        }

        // IO Actions
        0x10 => {
            // submit async io op
            let handle = Handle::new(registers.ebx as usize);
            let op_ptr = registers.ecx as *const AsyncOp;
            let op = unsafe { &*op_ptr };
            let wake_set = match registers.edx {
                0xffff_ffff => None,
                edx => Some(Handle::new(edx as usize)),
            };
            match actions::io::send_io_op(handle, op, wake_set) {
                Ok(_) => registers.eax = 1,
                // TODO: error codes
                Err(_e) => registers.eax = 0x8000_0000,
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
                Err(_e) => {
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
