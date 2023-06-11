.intel_syntax noprefix
.code32
.global start

start:
  mov eax, 0xffff
  mov ebx, 0x0b
  mov ecx, 0x0c
  int 0x2b

loop:
  jmp loop
