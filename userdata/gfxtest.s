.intel_syntax noprefix
.code16
.global start

start:
  # Set mode 13h (320x200x256)
  mov ax, 0x0013
  int 0x10

  # Fill the whole screen with color 1 (blue)
  mov ax, 0xA000
  mov es, ax
  xor di, di
  mov cx, 32000       # 320*200/2 = 32000 words
  mov ax, 0x0101      # color 1 in both bytes
  rep stosw

  # Draw a big red rectangle in the center
  # row 60, col 80, 160x80
  mov di, 60*320+80
  mov cx, 80          # 80 rows
red_rows:
  push cx
  push di
  mov cx, 160         # 160 pixels wide
  mov al, 4           # color 4 (red)
  rep stosb
  pop di
  add di, 320         # next row
  pop cx
  loop red_rows

  # Draw a white rectangle
  # row 80, col 100, 120x40
  mov di, 80*320+100
  mov cx, 40
white_rows:
  push cx
  push di
  mov cx, 120
  mov al, 15          # color 15 (white)
  rep stosb
  pop di
  add di, 320
  pop cx
  loop white_rows

  # Wait for a keypress via DOS
  mov ah, 0x01
  int 0x21

  # Return to text mode
  mov ax, 0x0003
  int 0x10

  # Exit
  mov ah, 0x00
  int 0x21

  jmp $
