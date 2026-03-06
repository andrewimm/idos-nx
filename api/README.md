# `idos_api` — Stdlib for native IDOS applications

A `no_std` userspace library that provides the interface between IDOS applications and the kernel. Syscalls are issued via `INT 0x2B`.

## Modules

- **`syscall`** — Raw syscall wrappers organized by category:
  - `exec` — Task management (create, terminate, yield), futex wait, executable loading, Virtual 8086 mode entry
  - `io` — Handle creation (file, message queue, pipe, IRQ, wake set), async I/O submission, filesystem/device driver registration
  - `memory` — Virtual memory mapping (`mmap`-style), physically contiguous/DMA allocation, file mapping, unmapping
  - `pci` — PCI bus access
  - `time` — Time-related syscalls
- **`io`** — Higher-level I/O abstractions:
  - `AsyncOp` — Async operation descriptors passed to the kernel for read/write/open/close
  - `Handle` — Opaque kernel handle wrapper
  - `Message` — Fixed-size IPC message type (also used for async message reads)
  - `driver` — Driver-side I/O helpers
  - `file` — File operations
  - `error` — I/O error types
  - `sync` — Synchronous I/O utilities
  - `termios` — Terminal I/O settings
- **`ipc`** — Interprocess communication via message passing (8×u32 message structs)
- **`compat`** — `VMRegisters` struct for Virtual 8086 mode interop
- **`time`** — Time utilities
