# IDOS NX
The next iteration of an experimental DOS-compatible OS

https://github.com/andrewimm/idos-nx/assets/494043/4c86f739-f5ea-4692-ac50-f1565cd940c5

The original [IMM-DOS](https://medium.com/@andrewimm/writing-a-dos-clone-in-2019-70eac97ec3e1)
was an experiment in building a strict DOS clone: a real-mode OS for x86
processors that implemented the same API surface as DOS and imitated many of
its behaviors. **NX** takes it a significant step further, implementing a
protected-mode OS that runs real-mode DOS programs in Virtual 8086
environments.

IDOS NX is a multitasking 32-bit OS. Generally speaking, it is designed to
support technologies that would have been present in a mid-90s / early-200s era
PC. While it is a serious attempt to learn the ins and outs of writing an
actually-usable, modern-ish operating system, it shouldn't be taken too
seriously. This is never going to be your daily driver, and it's not meant to
replace anything -- this is just an example of building something for the sake
of learning, like anything else I do. It's a fun what-if exercise exploring how
DOS might have evolved into he 90s, if not for the advent of Windows.

## Design and Architecture

At the core of the OS is the kernel: a multitasking 32-bit protected mode
kernel that can spin up Virtual 8086 (VM86) environments to emulate a real mode
DOS environment. Oh, and it's written in Rust.

For now, most of the OS functions are compiled into the kernel, although they
are decoupled from the core. The plan is to migrate most drivers and tasks into
standalone programs that can run in user mode, over time. The goal is a more
resilient OS: DOS and the pre-NT Windows versions were vulnerable to crashing
when a single driver misbehaved. With tasks that don't touch kernel internals
directly, and instead rely on message passing and memory sharing, it should be
possible to crash a driver and bring it back up without too much disruption.

### Tasks

As mentioned, the kernel is multitasking. Programs run in isolated spaces
called Tasks. Each Task has its own unique page table, making it easier for
programs to co-exist without accidentally clobbering each other. Tasks can
communicate via message-passing IPC, and can also share memory ranges with each
other.

There is no kernel-level concept of threads. Multithreading can be implemented
in user-space instead.

### Drivers

The multitasking kernel runs drivers as daemon tasks. Drivers may provide
device or filesystem access. Device drivers provide read/write access to
hardware like disks and peripherals, while filesystems implement file IO
semantics, often on top of a block device. Installable drivers make it easier
to extend support for different classes of hardware. They also reduce the size
and complexity of the kernel's core.

User mode drivers communicate with the kernel through the Async Arbiter. This
task enqueues requests to different drivers, sends them out via message-passing
IPC, and handles the response. Operating at the kernel level, the Arbiter
negotiates memory sharing between the originating task and the driver, so that
requests can be sent and responses can be written.

Kernel drivers can also install DOS interrupt hooks. It was common for users
to extend DOS functionality by calling into drivers using software interrupts.
A driver can specify which entry in the DOS Interrupt Vector Table should be
used, as well as a stub of code which will be copied into the DOS environment.
(Some drivers, like the Network Packet Driver spec, read the code from
installed interrupt handlers to find the right interrupt number. This means
drivers may need to inject code to be recognized by calling code.)

### Filesystem

Filesystems aren't just for disks anymore! The NX kernel uses virtual
filesystems to expose access to devices and OS internals as well as regular
storage. While this may seem like a Unix-y shift, it's important to remember
that MS-DOS originally used special filenames like `COM1`, `CON`, `AUX`, or
`PRN`. Virtual filesystems lead to better organization of these special files
and make debugging the OS much easier.

IDOS preserves the `C:\` style of representing absolute file paths, but allows
drive names to be more than one character long. Device files are located under
the `DEV:` filesystem, and information on current tasks is found in `TASK:`.
Physical or emulated disk drives are encouraged to use single-letter names,
though, since only those disks will be available to DOS programs.

### Memory

The system is implemented as a higher-half kernel -- it exists above
`0xc0000000` in virtual memory. This allows usermode programs to use the lower
3/4 of virtual memory without needing to switch page tables each time a syscall
is triggered. This also makes it easier for the kernel to reference memory in
the current task.

Each task has its own mapping for the lower 3GiB. All tasks share the same
mappings for kernel code and heap, so any task can execute a syscall. Each task
also has its own kernel-mode stack to allow for preemptive multitasking.

Task separation is less about security, and more about system stability (no
Guru Meditations!). Any task may be granted access to another task's memory
through syscalls, and this can be used to facilitate inter-process cooperation.
The idea is that this behavior is opt-in, though. Programs will not
unintentionally mess with another program's memory space.

When native 32-bit programs are run, they are loaded into lower memory with a
pre-allocated stack at the top of user space. Arbitrary ranges can be mapped
to files, hardware, or memory using mmap-style syscalls.

When a DOS program is run, a task is created with a simple linear memory area
resembling that of a 16-bit PC. Room is carved out for low-level x86 functions
like the Interrupt Vector Table, as well as some of the DOS internals that
programs like to poke into. A single DOS PSP is placed above the reserved space
and the executable code is copied above that.

If a DOS program attempts to directly access BIOS ROM or VGA memory, simulated
versions of these are mapped to the appropriate areas, preserving the illusion
that the program is running on a 16-bit PC.

### DOS Virtualization

When the system loads a DOS executable, it creates a new Virtual 8086 VM using
behavior built into the x86 protected mode. This container uses virtual memory
to create the appearance of an entire PC memory area. Attempting to run
privileged instructions will trap the kernel; the kernel will inspect the cause
and emulate the expected behavior before returning to the DOS program.

An example is the DOS API, called using `int 0x21`. When a program calls this
software interrupt, the kernel will intercept it and use the CPU registers to
determine which method is being called. Something like a file operation might
be proxied to the kernel's virtual filesystem, eventually transiting through a
filesystem driver before returning the result to the DOS program.

### Booting

In addition to an all-Rust kernel, IDOS also starts from disk using a chain of
bootloaders that are also written in Rust:

- The 512-byte MBR (at `bootloader/mbr/`) contains the earliest code that runs
after BIOS has initialized the system. This serves to load the second stage
bootloader (BOOTBIN)

- BOOTBIN (at `bootloader/bootbin`) performs the actual loading of the kernel
from disk into memory. It contains just enough code to perform some system
initialization, read some of the data from the FAT FS, and copy the kernel
bytes to the 1MiB mark. Once ready, it jumps into the kernel code.

## NX?
NX is the **N**E**X**T generation kernel, after the original real mode IMM-DOS.
It shares no code with that OS, and represents a technological leap forward in
capabilities and user experience. Any similarities to other two-letter kernels
starting with "N" are purely coincidental.

## Features and Goals

Beyond functioning as a bootable OS for x86 systems, there are a few defining
goals for what the project seeks to accomplish:

 - 32-bit kernel and drivers
 - Ability to run DOS COM and EXE programs in a VM
 - User-mode device and filesystem drivers
 - Multitasking with terminal multiplexing
 - System internals exposed through virtual drives (`DEV:`, `TASK:`, etc)

There are also a number of ambitious longer-term goals that extend the
capabilities of the system:

 - VGA/VESA Framebuffer drivers
 - SoundBlaster 16 driver
 - Networking (exposed to DOS using the "Packet Driver Spec")
 - System-wide support for UTF-8
 - DOS Protected Mode Interface (DPMI)
 - An Expanded Memory Manager (EMM) that complies with Microsoft's Global EMM
 Import spec, allowing programs to use more than 640KiB of memory.

This OS is explicitly **not** intended to be a POSIX system. At some point
during its earliest development, it became apparent that each tacked-on feature
was becoming just another UNIX-like. In order to make this more unique and give
it a clearer purpose, these guidelines were formalized.

## Building and Running

Building the kernel relies solely on nightly Rust.
The kernel currently builds with the **1.66.0-nightly** toolchain.

Building the rest of the boot disk uses a few more GNU Development Tools like
`objcopy` and `make`. If you have GNU devtools, you should be good.

The disk image itself is a FAT-12 filesystem, and uses `mtools` to format the
disk and copy files.

To summarize, you need:
 - Rust Nightly (1.66.0)
 - GNU development tools (`objcopy`, `make`)
 - `mtools`

To build the system disk, run `make` from the root directory.
To run the result in QEMU, run `make run`.

## Notes

This code is for demonstration purposes, and is licensed under the terms found
in the LICENSE file in the root of this repository.

I am providing code in the repository to you under an open source license.
Because this is my personal repository, the license you receive to my code is
from me and not my employer (Meta Platforms).
