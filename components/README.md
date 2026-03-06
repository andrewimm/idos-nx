# components — Userspace OS Components

All IDOS components that run outside the kernel, built as ELF binaries targeting `i386-idos.json`.

## drivers/

- **`fatdriver`** — FAT12 filesystem driver
- **`e1000`** — Intel E1000 Ethernet driver

## programs/

- **`command`** — Command shell (batch files, environment, lexer/parser)
- **`elfload`** — ELF executable loader
- **`doslayer`** — DOS compatibility layer (INT 21h API emulation)
- **`gfx`** — Graphics driver
- **`diskchk`** — Disk check utility
- **`colordemo`** — Terminal color demo
- **`netcat`** — Network cat utility
- **`gopher`** — Gopher protocol client
- **`gamedemo`** — Game demo
