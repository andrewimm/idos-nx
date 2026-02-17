//! setjmp/longjmp implementation for i386.
//!
//! jmp_buf layout: [EBX, ESI, EDI, EBP, ESP, EIP] (6 x u32 = 24 bytes)

use core::arch::global_asm;

global_asm!(
    r#"
.intel_syntax noprefix

.global setjmp
.global longjmp

setjmp:
    mov eax, [esp + 4]      /* eax = jmp_buf pointer */
    mov [eax],      ebx
    mov [eax + 4],  esi
    mov [eax + 8],  edi
    mov [eax + 12], ebp
    lea ecx, [esp + 4]      /* caller's esp (before call pushed return addr) */
    mov [eax + 16], ecx
    mov ecx, [esp]           /* return address */
    mov [eax + 20], ecx
    xor eax, eax             /* return 0 */
    ret

longjmp:
    mov edx, [esp + 4]      /* edx = jmp_buf pointer */
    mov eax, [esp + 8]      /* eax = val */
    test eax, eax
    jnz 1f
    inc eax                  /* if val == 0, return 1 */
1:
    mov ebx, [edx]
    mov esi, [edx + 4]
    mov edi, [edx + 8]
    mov ebp, [edx + 12]
    mov esp, [edx + 16]
    jmp [edx + 20]          /* jump to saved return address */
"#
);
