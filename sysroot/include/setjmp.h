#ifndef _SETJMP_H
#define _SETJMP_H

/* jmp_buf: EBX, ESI, EDI, EBP, ESP, EIP */
typedef int jmp_buf[6];

int setjmp(jmp_buf env);
void longjmp(jmp_buf env, int val) __attribute__((noreturn));

#endif
