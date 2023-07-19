use crate::interrupts::stack::StackFrame;
use crate::memory::address::VirtualAddress;

use super::syscall::dos_api;

/// When an interrupt occurs in VM86 mode, the stack pointer and segment
/// registers are pushed onto the stack before the typical stack frame.
#[repr(C, packed)]
pub struct VM86Frame {
    pub sp: u32,
    pub ss: u32,
    pub es: u32,
    pub ds: u32,
    pub fs: u32,
    pub gs: u32,
}

impl VM86Frame {
    pub fn get_stack_address(&self) -> VirtualAddress {
        VirtualAddress::new((self.ss << 4) + self.sp)
    }
}

impl core::fmt::Debug for VM86Frame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let sp = self.sp;
        let ss = self.ss;
        let es = self.es;
        let ds = self.ds;
        let fs = self.fs;
        let gs = self.gs;
        f.write_fmt(
            format_args!(
                "VM86Frame {{\n  sp: {:#010X}\n  ss: {:#010X}\n  es: {:#010X}\n  ds: {:#010X}\n  fs: {:#010X}\n  gs: {:#010X}\n}}\n",
                sp,
                ss,
                es,
                ds,
                fs,
                gs,
            )
        )
    }
}

///
#[repr(C, packed)]
pub struct DosApiRegisters {
    pub ax: u32,
    pub bx: u32,
    pub cx: u32,
    pub dx: u32,

    pub si: u32,
    pub di: u32,
    pub bp: u32,
}

impl DosApiRegisters {
    pub fn ah(&self) -> u8 {
        ((self.ax & 0xff00) >> 8) as u8
    }

    pub fn al(&self) -> u8 {
        (self.ax & 0xff) as u8
    }

    pub fn set_al(&mut self, al: u8) {
        self.ax &= 0xff00;
        self.ax |= al as u32;
    }

    pub fn dh(&self) -> u8 {
        ((self.dx & 0xff00) >> 8) as u8
    }

    pub fn dl(&self) -> u8 {
        (self.dx & 0xff) as u8
    }
}

/// When a DOS program running in VM86 mode tries to do something privileged,
/// it will trigger a GPF. If the kernel GPF handler determines that the source
/// was in a VM, it calls this method to handle the emulation logic.
pub fn handle_gpf(stack_frame: &StackFrame) -> bool {
    let stack_frame_ptr = stack_frame as *const StackFrame as usize;
    let vm_frame_ptr = (stack_frame_ptr + core::mem::size_of::<StackFrame>()) as *mut VM86Frame;
    let reg_ptr = (
        stack_frame_ptr - core::mem::size_of::<u32>() - core::mem::size_of::<DosApiRegisters>()
    ) as *mut DosApiRegisters;
    unsafe {
        let regs = &mut *reg_ptr;
        let vm_frame = &mut *vm_frame_ptr;
        let mut op_ptr = ((stack_frame.cs << 4) + stack_frame.eip) as *const u8;
        loop {
            // handle multi-byte instructions with prefixes
            if *op_ptr == 0x2e {
                // CS prefix
            } else if *op_ptr == 0x3e {
                // DS prefix
            } else if *op_ptr == 0x26 {
                // ES prefix
            } else if *op_ptr == 0x36 {
                // SS prefix
            } else if *op_ptr == 0x65 {
                // FS prefix
            } else if *op_ptr == 0x64 {
                // GS prefix
            } else if *op_ptr == 0x66 {
                // 32-bit op
            } else if *op_ptr == 0x67 {
                // 32-bit addr
            } else if *op_ptr == 0xf2 {
                // REPNZ
            } else if *op_ptr == 0xf3 {
                // REP
            } else {
                break;
            }

            // if there was a prefix
            op_ptr = op_ptr.add(1);
        }
        
        // TODO: handle PIO commands
        if *op_ptr == 0x9c {
            // PUSHF
            vm_frame.sp = vm_frame.sp.wrapping_sub(2) & 0xffff;
            *vm_frame.get_stack_address().as_ptr_mut::<u16>() = stack_frame.eflags as u16;
            stack_frame.add_eip(1);
            return true;
        } else if *op_ptr == 0x9d {
            // POPF
            let flags = *vm_frame.get_stack_address().as_ptr_mut::<u16>();
            vm_frame.sp = vm_frame.sp.wrapping_add(2) & 0xffff;
            stack_frame.set_eflags((flags as u32) | 0x20200);
            stack_frame.add_eip(1);
            return true;
        } else if *op_ptr == 0xcd {
            // INT
            let irq = *op_ptr.add(1);
            handle_interrupt(irq, regs, vm_frame, stack_frame);
            stack_frame.add_eip(2);
            return true;
        } else if *op_ptr == 0xcf {
            // IRET
        } else if *op_ptr == 0xf4 {
            // HLT
        } else if *op_ptr == 0xfa {
            // CLI
        } else if *op_ptr == 0xfb {
            // STI
        }
    }

    false
}

fn handle_interrupt(irq: u8, regs: &mut DosApiRegisters, vm_frame: &mut VM86Frame, stack_frame: &StackFrame) {
    match irq {
        // So many interrupts to implement here...
        
        0x21 => { // DOS API
            dos_api(regs, vm_frame, stack_frame);
        },

        // TODO: jump to the value in the IVT, or fail if there is no irq
        _ => panic!("Unsupported interrupt in VM86 mode {:X}", irq),
    }
}
