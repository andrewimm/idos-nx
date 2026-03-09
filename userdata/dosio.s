.intel_syntax noprefix
.code16
.global start

start:
  mov dx, offset message_prompt
  mov ah, 0x09
  int 0x21

  xor cl, cl
  readloop:
  mov ah, 0x01
  int 0x21
  mov dl, ' '
  mov ah, 0x02
  int 0x21
  inc cl
  cmp cl, 5
  jne readloop

  mov dx, offset message_done
  mov ah, 0x09
  int 0x21

  xor cx, cx
  auxloop:
  mov ah, 0x04
  mov bx, offset message_aux
  add bx, cx
  mov dl, [bx]
  int 0x21
  inc cx
  cmp cx, 8
  jne auxloop

  # Test INT 16h BIOS keyboard
  mov dx, offset message_kb_prompt
  mov ah, 0x09
  int 0x21

  xor cl, cl
  kbloop:
  # Poll with AH=01
  mov ah, 0x01
  int 0x16
  jz kbloop

  # Read with AH=00
  mov ah, 0x00
  int 0x16

  # Echo via INT 21h
  mov dl, al
  mov ah, 0x02
  int 0x21
  mov dl, ' '
  mov ah, 0x02
  int 0x21

  inc cl
  cmp cl, 5
  jne kbloop

  mov dx, offset message_kb_done
  mov ah, 0x09
  int 0x21

  mov ah, 0x00
  int 0x21

  jmp $

message_prompt: .ascii "Enter 5 characters: $"
message_done: .ascii "\nDONE.\n$"
message_aux: .ascii "DOS AUX\n"
message_kb_prompt: .ascii "BIOS KB test - type 5 keys: $"
message_kb_done: .ascii "\r\nBIOS KB DONE.\r\n$"
