# sysroot — C Cross-Compilation Sysroot

Sysroot for cross-compiling C programs to IDOS using GCC in 32-bit freestanding mode.

- **`include/`** — C standard headers (`stdio.h`, `stdlib.h`, `string.h`, `unistd.h`, `sys/stat.h`, etc.) and IDOS-specific headers (`idos/syscall.h`)
- **`lib/`** — Prebuilt `crt0.o` and `libc.a` (from the `libc/` crate)
- **`src/crt0.s`** — C runtime entry: clears frame pointer, inits x87 FPU, calls `__libc_init`, then `main(argc, argv)`, then `exit()`
- **`cmake/idos-toolchain.cmake`** — CMake toolchain file for cross-compilation (`-m32 -march=i386 -ffreestanding -nostdlib`)
- **`test/`** — Test C program
