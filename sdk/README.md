# idos_sdk — Runtime for Native IDOS Applications

`no_std` runtime crate for Rust programs targeting IDOS. Provides `_start` → `main()` entry, a global allocator, and common utilities.

- **`allocator`** — Slab allocator (6 size classes: 32–1024 bytes) backed by kernel page allocation
- **`env`** — `argc`/`argv` parsing and an `Args` iterator
- **`log`** — `SysLogger` for async logging to `LOG:\` endpoints
- **`panic`** — Panic handler that terminates the task
