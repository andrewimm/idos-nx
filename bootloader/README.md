# Bootloader

Two-stage bootloader for IDOS, written in Rust and x86 assembly. Targets real-mode i386 (`no_std`, custom target specs) and uses BIOS interrupts for disk I/O and video output.

## Stages

### `mbr/` — Stage 1 (MBR)

A 512-byte Master Boot Record loaded by the BIOS at `0x7C00`. It:

1. Sets up the stack and segment registers (in `boot.s`)
2. Parses the FAT12 BPB header to locate the root directory
3. Finds `BOOT.BIN` as the first root directory entry
4. Loads it into memory at `0x2000` and jumps to it, passing FAT metadata

The linker script places a FAT header at the start of the binary and the `0xAA55` boot signature at byte 510.

### `bootbin/` — Stage 2 (BOOT.BIN)

A larger second-stage bootloader loaded by the MBR. It:

1. Enables the A20 line (Fast A20 via port `0x92`)
2. Enters **unreal mode** — loads a GDT, briefly enters protected mode to cache 32-bit segment limits, then returns to real mode for continued BIOS access
3. Builds a memory map using BIOS `INT 15h, E820h` (stored at `0x1000`)
4. Finds and loads `KERNEL.BIN` from the FAT12 root directory, following the FAT12 cluster chain for fragmented files
5. Copies the kernel ELF to high memory at `0x100000`, validates the ELF header, processes section headers, and zeroes NOBITS sections
6. Optionally loads `FATDRV.BIN` (userspace FAT driver) to `0x20000`
7. Sets up an initial page directory and page table at `0x9000`/`0x8000`, identity-mapping the first 4 MiB and also mapping it at `0xC0000000` (where the kernel expects to live)
8. Enters protected mode, enables paging, and jumps to the kernel entry point

## Building

Both stages use custom target specs (`i386-mbr.json`, `i386-bootbin.json`) targeting `i386-unknown-none-code16` with `rust-lld` as the linker. Build with:

```
cargo build --release --target <target-spec>.json
```
