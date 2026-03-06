# userdata — A: Floppy Disk Contents

Test programs and static files copied onto the A: floppy image mounted by QEMU.

- **`static/`** — Files copied as-is (`hello.txt`, `DEMO.BAT`)
- **`testbin.s`** — Flat binary test program
- **`dosio.s`** — DOS-style `.COM` program (linked at `0x100`)
- **`elftest.c`** / **`crt0.s`** — Minimal ELF test executable
- **`Makefile`** — Builds everything into `disk/`
