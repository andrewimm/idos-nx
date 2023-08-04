.intel_syntax noprefix
.global _start

.text

_start:
  xor ebp, ebp
  xor eax, eax
  mov edi, [esp]
  lea esi, [esp + 4]
  push esi
  push edi
  call main
