.intel_syntax noprefix
.global _start
.extern main
.extern __libc_init
.extern exit

.text

_start:
    /* Clear frame pointer for clean backtraces */
    xor ebp, ebp

    /* Initialize x87 FPU */
    finit

    /* argc and argv are on the stack, set up by elfload */
    mov edi, [esp]
    lea esi, [esp + 4]

    /* Initialize libc (allocator, stdio, etc.) */
    call __libc_init

    /* Call main(argc, argv) */
    push esi
    push edi
    call main
    add esp, 8

    /* Call exit(main's return value) */
    push eax
    call exit

    /* Should never reach here */
    ud2
