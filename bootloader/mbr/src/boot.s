.section .boot, "ax"
.global _start
.code16

_start:
  # dl will be set to the current disk number
  
  # init the stack, segments
  mov sp, 0x7c00
  xor ax, ax
  mov ss, ax
  mov ds, ax
  mov es, ax
  # ensure cs is set to zero
  jmp 0x0000, offset boot_continue

boot_continue:
  cld

  push dx   # push the disk number as the first arg
  call mbr_start

