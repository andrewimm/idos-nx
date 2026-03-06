# libc — C Standard Library for IDOS

Minimal C standard library built as a Rust static library (`staticlib`), linked into C programs targeting IDOS. Implements POSIX/C functions on top of `idos_api` syscalls.

Modules: `stdio`, `stdlib`, `string`, `unistd`, `stat`, `dirent`, `mman`, `math`, `ctype`, `errno`, `locale`, `signal`, `setjmp`, `termios`, `time`, and a custom `allocator`.

Initialized by `__libc_init()`, called from `crt0.s` before `main()`.
