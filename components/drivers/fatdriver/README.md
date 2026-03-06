# fatdriver — FAT12 Filesystem Driver

Userspace FAT12 filesystem driver for IDOS. Built as both a library (platform-independent, testable on the host) and a binary (runs as an IDOS task via `idos_api`/`idos_sdk`).

## Overview

The driver mounts a block device, registers itself as a filesystem (e.g. `C:`), and services file I/O requests via the kernel's async driver protocol. It supports open, read, write, close, stat, mkdir, rmdir, unlink, rename, and memory-mapped file page-in.

On startup it reads the drive letter and block device name from a pipe, opens the device, parses the BPB from the boot sector, and enters a message loop.

## Modules

- **`bpb.rs`** — BIOS Parameter Block parsing (bytes/sector, sectors/cluster, reserved sectors, FAT count, root directory entries, sectors/FAT)
- **`table.rs`** — FAT12 allocation table: cluster chain traversal, allocation, freeing, and read/write of 12-bit FAT entries across all FAT copies
- **`dir.rs`** — Directory entry structures and operations:
  - `RootDirectory` / `SubDirectory` / `AnyDirectory` — iteration, find, add, remove entries
  - 8.3 filename matching (case-insensitive), `FileTime`/`FileDate` encoding/decoding
  - `File` — cluster-chain-cached reads and writes with automatic cluster allocation on write
  - Path resolution through nested subdirectories
- **`disk.rs`** — `DiskIO` trait and `DiskAccess` sector cache:
  - Hash-table-indexed LRU cache with dirty tracking and write-back on eviction/flush
  - 16-sector readahead on cache miss
  - Byte-level read/write/struct serialization over the sector abstraction
- **`fs.rs`** — `FatFS`: ties together BPB, allocation table, and disk access
- **`driver.rs`** — `FatDriver`: handle management, file/directory operations, memory-mapped file support with refcounted mapping tokens
- **`main.rs`** — IDOS entry point: reads config from startup pipe, creates `IdosDiskIO` (backed by block device syscalls), registers as a filesystem, runs the message loop

## Building

The library builds on the host for testing (`cargo test`). The binary requires the `idos` feature and targets IDOS with a custom linker script (`link-script.ld`, loaded at `0x400000` as a flat binary with a trampoline entry).

```
cargo test                                        # host tests
cargo build --release --features idos --target ... # IDOS binary
```
