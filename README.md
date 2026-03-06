# IDOS NX

A 32-bit operating system that asks: what if DOS never died?

IDOS-NX is a from-scratch OS written in Rust that takes the DOS philosophy -
boot fast, run programs, get out of the way - and drags it kicking and
screaming into protected mode. It's not DPMI. It's not POSIX-compatible. It's
what might have been if someone kept iterating on DOS instead of replacing it.

It boots from a custom bootloader into a mini-kernel (I want to say microkernel
but we're not quite there yet). Instead of a single-tasking text-mode command
line, you get a tiling window manager with multiple desktops. It has
networking. It can run 16-bit DOS programs using Virtual 8086 mode. It runs all
its drivers in 32-bit protected mode. If a driver crashes, theoretically it can
keep on running.

Honestly this is the third or fourth iteration of this thing. The first one
started to get too Unix-y. That was boring. It needed to be different. The
second one hit issues when I started trying to block on multiple devices at the
same time. Rather than implement something like select/poll, I went
async-first all the way through the kernel. After multiple iterations of that,
I found something I actually like. So far, so good.

## What Works Today

Kernel
- 32-bit protected mode, higher half at 0xC0000000
- Multitasking, with per-task memory spaces
- Multi-core support
- Async-first I/O with wake sets (think kqueue, but for DOS)
- Virtual filesystems - drives instead of mount points, but we've move past 26
  letters into real drive names

Desktop
- Supports multiple resolutions, VESA graphics with a compositing window manager
- Tiled and floating windows that you can drag around
- Use a IOCTL to turn your terminal into a graphics buffer
- PS/2 mouse support, with stylish cursor
- 24-bit colors, windows can use lower color depth for palette effects
- Console Manager has a cool name: `CONMAN`

Shell (`COMMAND.ELF`)
- Basically `COMMAND.COM` but 32 bits
- DIR, CD, COPY, DEL, MOVE, REN, TYPE, and friends
- Batch script execution (`.BAT` files)
- Output redirection
- Ctrl-C terminates rogue programs (but not the shell, I fixed that)

Filesystem
- FAT12 via a userspace driver
- Drive names, with one or more letters: `C:` for your HDD, `A:` for your
  floppy. But we also have `DEV:` for devices, `SYS:` for system properties,
  `TASK:` for browsing processes, and more

Networking
- Intel E1000 ethernet (userspace driver)
- TCP/UDP sockets, ARP, DHCP, DNS
- Semi-working Gopher browser and netcat, just to prove it works

DOS Compatibility
- VM86 mode for running real 16-bit DOS COM/EXE programs
- INT 21h calls trapped and proxied to the kernel's 32-bit filesystem
- VGA memory and BIOS ROM virtualized

## What makes IDOS different?

Drivers run in userspace, for safety. The FAT driver, ethernet driver, graphics
driver, and more are all separate ELF binaries that load at boot time from a
config file. If a driver crashes, the kernel doesn't (that's right, I'm calling
you out Win95). But it's the kernel's decision whether to start your broken
driver again.

All I/O is async-first and everything is a handle. Files, devices, pipes,
sockets, child processes - they're all handles, and every operation on them is
non=blocking at the kernel level. If you want sync IO, just issue an async call
and park your task on a futex until the kernel signals completion.

Wake Sets allow you to wait on multiple things at once. Create a wake set,
attach handles to it, and block until any of them has something you want. The
console manager uses one to simultaneously wait on keyboard input, mouse data,
driver messages, child process exits - all from a single-threaded event loop.
It's `select(2)` if `select(2)` didn't make you want to quit programming.

Under the hood, signalling is built on futexes -- very futuristic. The kernel
wakes you when the value at an address changes. Async operations store their
completion status in a futex, and wake sets aggregate multiple futexes into a
single wait point.

There are no kernel threads. There is no built-in async runtime. Your programs
are single-threaded and if you want green threads or an executor or whatever,
you build it yourself on top of these primitives.

Every program has its own address space, but it can mess around with anyone
else's. Security? What's that? Our goal is to not crash, but the cool thing
about DOS was that you could modify other programs. This is mostly used for
tame things like memory sharing, but imagine what else you could do.

It's not trying to be Unix. No `/`, no inodes, no fork/exec. Drives and
backslashes. `DEV:\CON1` instead of `/dev/tty`. The path separator is `\` and
I'm only partly sorry about that.

## Program Execution

To start a program, you create a Task, you add some args / env variables, and
then you attach an executable. Based on your file type, the kernel injects
different loaders into the address space, which are responsible for actually
loading the program. This is how it can seamlessly load native ELF programs, as
well as 16-bit DOS COM/EXE files.

Memory frames are shared, when possible, so it's pretty cheap to share a loader
between different tasks. Same with shared libraries - just load them into local
address space, and let the loader fix up addresses.

As mentioned above, programs are always single-threaded, but the OS supports
multi-core systems, so you can run your application and the disk driver at the
same time.

## FAQ

### Why DOS? It's 2026 (or possibly later, if I haven't updated the README)

DOS gota lot of things right. Programs own the machine. The API is small. Boot
is fast. The mistake was building it on real mode and single-tasking, not the
philosophy. We kept the philosophy and left the 8086 behind.

### Can it run DOOM?

It's complicated, but yes. I've forked Chocolate DOOM to run with the system's
unique properties. It sends the right IOCTL to turn a terminal into a graphics
buffer, gets keyboard input, and plays a nice game.

It would be nice if DOS emulation were far enough along (DPMI and more) to run
OG DOOM, but we're still getting there. Maybe one day.

### Do you have libc?

Most of one. This is how DOOM works. The meat is in Rust, with a FFI API, using
the native syscall interface. Using the headers, you can actually run other
programs on this system.

### Is this production-ready?

We don't even have a stable way to shut down. You close the QEMU window. So no.

### How do you debug this thing?

We dump a lot of serial output to COM1. So much that we have our own log filter
program (`make runlogs`). Nothing gets better than `kprintln!` debugging
though. QEMU also has a nice GDB interface for when things get gnarly.

### Why drive letters/names?

Because `C:\COMMAND.ELF` has more personality than `/usr/bin/sh`. Also I
genuinely didn't want to implement a VFS with arbitrary mount points. Drive
letters are a flat namespace, and flat namespaces are easy.

### Why are drivers in userspace? Isn't that slower?

So when we write a bad ethernet driver -- and we *will* write a bad ethernet
driver -- it crashes in its own address space instead of taking the kernel down
with it. Also it means drivers are just binaries loaded from a config file at
boot, and isn't that really the DOS way?

### Can I contribute?

You're looking at an OS that boots from a hand-rolled FAT bootloader into a
tiling window manager with networking, and the entire kernel has no unsafe-free
guarantees whatsoever. If that excites rather than horrifies you, sure.

### Do you have tests?

Actually, yeah. Unit testing a kernel is awkward, but it can boot into a mode
that just runs test cases with few-to-no drivers.

### Your bootloader parses FAT in 512 bytes?

510 bytes, technically. Two are the boot signature. It was tricky, and honestly
it's probably not good x86 assembly, but it works. I'm a little proud.

### You implemented $FEATURE poorly

That's not a question. But yeah you're probably right. I'm learning.

### What's next?

More windows, more programs. Improved DOS support with DPMI, which is the way a
protected-mode OS is supposed to run DOS programs and give them access to more
memory. MS created an EMM spec too, for programs to ask for more memory. That'd
be cool. Maybe I'll just add more Gopher browsing features. The roadmap is
"whatever seems fun on a Saturday afternoon."

## Building

Requires Rust nightly, `mtools`, `objcopy`, `make`. Runs in QEMU. Don't try on
real hardware that you actually care about.

```sh
make
make run

# or if you want fancy log filtering

make runlogs
```

## Status

This is a hobby OS. It will eat your data, forget your tasks, and occasionally
triple-fault into the shadow realm. But it boots quickly and the window manager
is surprisingly usable.

## NX?
NX is the **N**E**X**T generation kernel, after the original real mode IMM-DOS.
It shares no code with that OS, and represents a technological leap forward in
capabilities and user experience. Any similarities to other two-letter kernels
starting with "N" are purely coincidental.

