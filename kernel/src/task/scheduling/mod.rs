//! Task scheduling with a single global work queue shared across all CPUs.
//! Each CPU has its own CPUScheduler for per-core state (current task, GDT,
//! LAPIC, tick counting), but all runnable tasks live in one global queue.

use core::arch::asm;
use core::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use alloc::collections::VecDeque;
use spin::Mutex;

use crate::{
    arch::gdt::{GdtEntry, TssWithBitmap},
    hardware::lapic::LocalAPIC,
    memory::{
        address::{PhysicalAddress, VirtualAddress},
        physical::allocate_frame,
    },
};

use super::{
    id::{AtomicTaskID, TaskID},
    map::get_task,
    paging::{current_pagedir_map, current_pagedir_map_explicit, PermissionFlags},
    stack::KERNEL_STACKS_BOTTOM,
    switching::switch_to,
};

/// All cores pull tasks from this queue. At 8 cores or less, we should avoid
/// too much lock contention
static GLOBAL_WORK_QUEUE: Mutex<VecDeque<WorkItem>> = Mutex::new(VecDeque::new());

/// Pointers to all CPUScheduler instances, for collecting per-CPU stats.
static CPU_SCHEDULERS: Mutex<alloc::vec::Vec<SchedulerPtr>> =
    Mutex::new(alloc::vec::Vec::new());

/// Wrapper around a raw pointer to CPUScheduler that is Send+Sync.
/// Safety: CPUScheduler instances are allocated once per CPU and never freed.
struct SchedulerPtr(*const CPUScheduler);
unsafe impl Send for SchedulerPtr {}
unsafe impl Sync for SchedulerPtr {}

/// This struct is instantiated once per CPU core, and manages data necessary to
/// run and switch tasks on that core.
#[repr(C)]
pub struct CPUScheduler {
    linear_address: VirtualAddress,
    cpu_index: usize,
    current_task: AtomicTaskID,
    idle_task: TaskID,

    pub gdt: [GdtEntry; 9],

    pub has_lapic: bool,
    current_ticks: AtomicU8,

    /// Task ID to re-enqueue after a context switch completes, stored as raw
    /// u32. 0xFFFFFFFF means "none". Written before the asm switch, consumed
    /// after — survives the stack swap because it's per-CPU, not on the stack.
    pending_reenqueue: AtomicU32,

    /// Per-CPU TSS. Each core needs its own so that hardware interrupts load
    /// the correct ring-0 stack pointer (ESP0) for whatever task that core is
    /// running.
    pub tss: TssWithBitmap,

    /// Per-CPU tick counters for CPU time accounting.
    user_ticks: AtomicU32,
    kernel_ticks: AtomicU32,
    idle_ticks: AtomicU32,
}

impl CPUScheduler {
    pub fn new(cpu_index: usize, idle_task: TaskID, linear_address: VirtualAddress) -> Self {
        let mut gdt = unsafe { crate::arch::gdt::GDT.clone() };
        gdt[5].set_base(linear_address.as_u32());

        Self {
            linear_address,
            cpu_index,
            current_task: AtomicTaskID::new(idle_task.into()),
            idle_task,
            gdt,

            has_lapic: false,
            current_ticks: AtomicU8::new(0),
            pending_reenqueue: AtomicU32::new(0xFFFFFFFF),
            tss: TssWithBitmap::new(),

            user_ticks: AtomicU32::new(0),
            kernel_ticks: AtomicU32::new(0),
            idle_ticks: AtomicU32::new(0),
        }
    }

    pub fn load_gdt(&mut self) {
        self.tss.init(&mut self.gdt[7]);
        let mut gdtr = crate::arch::gdt::GdtDescriptor::new();
        gdtr.point_to(&self.gdt);
        gdtr.load();
        crate::arch::gdt::ltr(0x38);
    }

    pub fn set_tss_stack_pointer(&mut self, sp: u32) {
        self.tss.tss.esp0 = sp;
    }

    pub fn get_current_task(&self) -> TaskID {
        self.current_task.load(Ordering::SeqCst)
    }

    pub fn get_idle_task(&self) -> TaskID {
        self.idle_task
    }

    pub fn set_current_task(&self, id: TaskID) -> TaskID {
        self.current_task.swap(id, Ordering::SeqCst)
    }

    /// Record one tick of CPU time in the appropriate per-CPU bucket.
    pub fn record_tick(&self, is_user: bool) {
        let is_idle = self.get_current_task() == self.idle_task;
        if is_user {
            self.user_ticks.fetch_add(1, Ordering::Relaxed);
        } else if is_idle {
            self.idle_ticks.fetch_add(1, Ordering::Relaxed);
        } else {
            self.kernel_ticks.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn get_cpu_index(&self) -> usize {
        self.cpu_index
    }

    pub fn get_tick_counts(&self) -> (u32, u32, u32) {
        (
            self.user_ticks.load(Ordering::Relaxed),
            self.kernel_ticks.load(Ordering::Relaxed),
            self.idle_ticks.load(Ordering::Relaxed),
        )
    }

    /// Called every timer tick (10ms). Returns true if the current task's
    /// time slice has expired.
    pub fn tick(&self) -> bool {
        let prev = self.current_ticks.fetch_add(1, Ordering::Relaxed);
        if prev >= 1 {
            // 2 ticks = 20ms time slice (~50Hz)
            self.current_ticks.store(0, Ordering::Relaxed);
            return true;
        }
        false
    }
}

pub enum WorkItem {
    Task(TaskID),
    Tasklet(Tasklet),
}

pub struct Tasklet {}

pub fn create_cpu_scheduler(
    cpu_index: usize,
    idle_task: TaskID,
    has_lapic: bool,
) -> VirtualAddress {
    // Create an area of memory for the CPU's scheduler struct
    // This memory is only referenced by the scheduler itself, so it can be
    // directly allocated and should never be freed.
    let mapped_to = VirtualAddress::new((KERNEL_STACKS_BOTTOM - 0x2000 * (cpu_index + 1)) as u32);
    let frame = allocate_frame().unwrap();
    current_pagedir_map(frame, mapped_to, PermissionFlags::empty());

    unsafe {
        let scheduler_ptr = mapped_to.as_ptr_mut::<CPUScheduler>();
        scheduler_ptr.write(CPUScheduler::new(cpu_index, idle_task, mapped_to));

        let scheduler = &mut *scheduler_ptr;
        scheduler.has_lapic = has_lapic;
        scheduler.load_gdt();

        CPU_SCHEDULERS.lock().push(SchedulerPtr(scheduler_ptr as *const CPUScheduler));
    }

    if has_lapic {
        // map the CPU's LAPIC to the page beyond the scheduler struct
        let lapic_mapping = mapped_to + 0x1000;

        let mut lapic_phys: u32;
        unsafe {
            let msr: u32 = 0x1b;
            core::arch::asm!("rdmsr", in("ecx") msr, out("eax") lapic_phys, out("edx") _);
        }
        lapic_phys &= 0xfffff000;
        current_pagedir_map_explicit(
            PhysicalAddress::new(lapic_phys),
            lapic_mapping,
            PermissionFlags::empty(),
        );
    }

    mapped_to
}

/// Get the CPUScheduler instance for the current CPU
pub fn get_cpu_scheduler() -> &'static mut CPUScheduler {
    // GS shouldn't be set here, but it's getting overridden by userspace
    // programs. We should probably set it whenever entering the kernel, or
    // when switching tasks.
    unsafe {
        let raw_addr: u32;
        asm!(
            "mov gs, {}",
            "mov {}, gs:[0]",
            in(reg) 0x28,
            out(reg) raw_addr,
        );
        let addr = VirtualAddress::new(raw_addr);

        &mut *addr.as_ptr_mut::<CPUScheduler>()
    }
}

pub fn get_lapic() -> LocalAPIC {
    unsafe {
        let raw_addr: u32;
        asm!(
            "mov gs, {}",
            "mov {}, gs:[0]",
            in(reg) 0x28,
            out(reg) raw_addr,
        );
        let addr = VirtualAddress::new(raw_addr + 0x1000);

        LocalAPIC::new(addr)
    }
}

/// Get the current task ID for the CPU that calls this function.
pub fn get_current_task_id() -> TaskID {
    get_cpu_scheduler().get_current_task()
}

/// Collect per-CPU tick counts: returns a Vec of (cpu_index, user, kernel, idle).
pub fn get_all_cpu_ticks() -> alloc::vec::Vec<(usize, u32, u32, u32)> {
    let schedulers = CPU_SCHEDULERS.lock();
    schedulers
        .iter()
        .map(|sp| unsafe {
            let s = &*sp.0;
            let (u, k, i) = s.get_tick_counts();
            (s.get_cpu_index(), u, k, i)
        })
        .collect()
}

/// Put a task on the global work queue, making it eligible for execution again.
pub fn reenqueue_task(id: TaskID) {
    GLOBAL_WORK_QUEUE.lock().push_back(WorkItem::Task(id));
}

/// Pop the next runnable task from the global work queue and switch to it.
/// The outgoing task is NOT re-enqueued until after the context switch saves
/// its state, preventing another core from running it while this core is
/// still on its stack.
pub fn switch() {
    let scheduler = get_cpu_scheduler();

    // Drain any unconsumed pending reenqueue left over from a prior switch
    // to an Initialized task. That path uses iretd (never returns through
    // switch_to), so the post-switch drain below never ran for it.
    let stale = scheduler
        .pending_reenqueue
        .swap(0xFFFFFFFF, Ordering::SeqCst);
    if stale != 0xFFFFFFFF {
        reenqueue_task(TaskID::new(stale));
    }

    let current_id = scheduler.current_task.load(Ordering::SeqCst);

    // Determine if the current task should be re-enqueued, but don't do it
    // yet — we must wait until after switch_to() saves the outgoing state.
    let should_reenqueue = if current_id != scheduler.idle_task {
        get_task(current_id)
            .map(|t| t.read().can_resume())
            .unwrap_or(false)
    } else {
        false
    };

    let switch_to_id = loop {
        // Pop into a local so the GLOBAL_WORK_QUEUE lock is released before
        // we touch get_task() / GLOBAL_TASK_MAP. Holding both simultaneously
        // inverts the lock order vs. task creation (which holds GLOBAL_TASK_MAP
        // then calls reenqueue_task → GLOBAL_WORK_QUEUE).
        let item = GLOBAL_WORK_QUEUE.lock().pop_front();
        match item {
            Some(WorkItem::Task(id)) => {
                if let Some(task_lock) = get_task(id) {
                    if task_lock.read().can_resume() {
                        break id;
                    }
                }
            }
            Some(WorkItem::Tasklet(_)) => panic!("Tasklet isn't supported"),
            None => {
                break scheduler.idle_task;
            }
        }
    };

    if current_id == switch_to_id {
        return;
    }

    // Store the outgoing task ID in per-CPU state. After the asm context
    // switch, we'll be on a different stack but the same CPU — read it
    // back and re-enqueue then.
    if should_reenqueue {
        scheduler
            .pending_reenqueue
            .store(current_id.into(), Ordering::SeqCst);
    }

    switch_to(switch_to_id);

    // We're now on the resumed task's stack. The outgoing task's state has
    // been fully saved. Safe to let another core run it.
    let scheduler = get_cpu_scheduler();
    let prev = scheduler
        .pending_reenqueue
        .swap(0xFFFFFFFF, Ordering::SeqCst);
    if prev != 0xFFFFFFFF {
        reenqueue_task(TaskID::new(prev));
    }
}
