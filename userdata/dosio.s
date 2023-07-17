.intel_syntax noprefix
.code16
.global start

start:
  mov dx, offset message_prompt
  mov ah, 0x09
  int 0x21

  jmp $

message_prompt: .ascii "Enter 5 characters: $"
