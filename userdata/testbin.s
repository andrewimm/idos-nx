.intel_syntax noprefix
.code32
.global start

start:
  mov eax, 0x0a
  mov ebx, 0x0b
  mov ecx, 0x0c

loop:
  jmp loop
