# e1000 — Intel E1000 Ethernet Driver

Userspace network driver for the Intel 82540EM (E1000) family of Ethernet controllers. Runs as an IDOS task using `idos_api` and `idos_sdk`.

## Overview

The driver receives PCI device info (BAR0, IRQ) via a startup pipe, maps the MMIO region, initializes the hardware, and registers itself as a device (`DEV:\ETH`) and network device with the kernel. It then enters an event loop, servicing driver I/O requests and hardware interrupts through a wake set.

## Modules

- **`controller.rs`** — Low-level MMIO register access (read/write/set/clear flags) and MAC address retrieval
- **`driver.rs`** — Hardware initialization (reset, link setup, interrupt mask, RX/TX descriptor rings) and packet TX/RX using DMA buffers
- **`main.rs`** — Event loop: dispatches `DriverCommand` messages (open, close, read, write) and handles IRQ-driven receive with deferred reads

## Hardware Details

- 8 RX and 8 TX descriptors, each backed by a 1024-byte DMA buffer
- Interrupts enabled for RX (link status change + RX timer, IMS = `0xC0`)
- RX control: unicast + multicast, 1024-byte buffer size, CRC strip
- TX control: enabled with short packet padding
