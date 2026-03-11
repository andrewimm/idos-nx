; Floppy MBR bootloader — CHS reads for floppy drives
; Loads BOOT.BIN (must be first root dir entry) to 0x2000 and jumps to it.
; Passes pointer to fat_metadata struct as first argument.
;
; BPB offsets from 0x7C00:
;   0x0D = sectors_per_cluster (byte)
;   0x0E = reserved_sector_count (word)
;   0x10 = fat_count (byte)
;   0x11 = max_root_dir_entries (word)
;   0x16 = sectors_per_fat (word)
;   0x18 = sectors_per_track (word)
;   0x1A = head_count (word)

bits 16
org 0x7C00

entry:
    jmp short _start
    nop

    ; BPB area (62 bytes from offset 3..0x3E) — will be overwritten by mkfs
    times 0x3E - ($ - $$) db 0

_start:
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    cld

    ; Save boot drive (BIOS DL)
    mov [boot_drive], dl

    ; root_dir_sector = reserved + fat_count * sectors_per_fat
    mov ax, [0x7C0E]        ; reserved_sector_count
    xor ch, ch
    mov cl, [0x7C10]        ; fat_count
.add_fat:
    add ax, [0x7C16]        ; + sectors_per_fat
    dec cx
    jnz .add_fat
    mov [fat_meta + 2], ax  ; store root_dir_sector

    ; root_cluster_sector = root_dir_sector + ceil(root_entries / 16)
    mov bx, [0x7C11]        ; max_root_dir_entries
    add bx, 15
    shr bx, 4
    add ax, bx
    mov [fat_meta + 4], ax  ; store root_cluster_sector

    ; sectors_per_cluster
    xor ah, ah
    mov al, [0x7C0D]
    mov [fat_meta], ax      ; store sectors_per_cluster

    ; Read 1 sector of root directory to 0x7E00
    mov ax, [fat_meta + 2]  ; root_dir_sector
    mov bx, 0x7E00
    mov cx, 1
    call read_sectors

    ; Verify first entry is "BOOT    BIN"
    mov si, boot_name
    mov di, 0x7E00
    mov cx, 11
    repe cmpsb
    jne .no_boot

    ; BOOT.BIN size -> sector count
    ; Size is at dir_entry + 28 (4 bytes), but we only need high word trick:
    ; sectors ~= size / 512. Size is at 0x7E1C (dword).
    ; [0x7E1D] is byte offset 29 = high byte of low word + low byte of high word
    ; Original MBR does: sectors = [0x7E1D as u16] >> 1, then +1
    mov ax, [0x7E1D]
    shr ax, 1
    inc ax
    mov cx, ax              ; sector count

    ; Load BOOT.BIN from root_cluster_sector to 0x2000
    mov ax, [fat_meta + 4]  ; root_cluster_sector
    mov bx, 0x2000
    call read_sectors

    ; Set up fat_metadata pointer and jump to BOOT.BIN
    ; Store disk_number in the struct
    mov al, [boot_drive]
    mov [fat_meta + 6], al

    ; BOOTBIN is compiled as 32-bit code (i386) — it expects dword args
    ; and a dword return address from calld. Match the hard disk MBR's
    ; calling convention: pushd arg, calld 0x2000
    push dword fat_meta     ; argument: pointer to fat_metadata (dword)
    push dword 0            ; dummy return address (BOOTBIN never returns)
    jmp 0x0000:0x2000

.no_boot:
    mov si, msg_no_boot
    call print_str
    jmp $

; -----------------------------------------------
; read_sectors
;   AX = starting LBA
;   BX = destination (ES:BX, ES=0)
;   CX = number of sectors
; Trashes AX, BX, CX, DX, SI
; -----------------------------------------------
read_sectors:
    mov si, ax              ; SI = current LBA
.rs_loop:
    jcxz .rs_done

    ; LBA-to-CHS
    mov ax, si              ; current LBA
    xor dx, dx
    div word [0x7C18]       ; AX = LBA/spt, DX = LBA%spt
    inc dl                  ; sector is 1-based
    push cx                 ; save remaining count
    mov cl, dl              ; CL = sector number

    xor dx, dx
    div word [0x7C1A]       ; AX = cylinder, DX = head
    mov ch, al              ; CH = cylinder low bits
    mov dh, dl              ; DH = head
    mov dl, [boot_drive]    ; DL = drive number

    mov ax, 0x0201          ; AH=02h read, AL=1 sector
    int 0x13

    pop cx                  ; restore remaining count
    inc si                  ; next LBA
    add bx, 512             ; next buffer position
    dec cx
    jmp .rs_loop

.rs_done:
    ret

; -----------------------------------------------
; print_str: Print null-terminated string at DS:SI
; -----------------------------------------------
print_str:
    lodsb
    test al, al
    jz .ps_done
    mov ah, 0x0E
    xor bx, bx
    int 0x10
    jmp print_str
.ps_done:
    ret

; -----------------------------------------------
; Data
; -----------------------------------------------
boot_name:   db "BOOT    BIN"
msg_no_boot: db "No BOOT.BIN", 0

boot_drive:  db 0

; fat_metadata struct: [u16 spc] [u16 root_dir_sec] [u16 root_cluster_sec] [u8 disk]
fat_meta:    dw 0, 0, 0
             db 0

; Pad to 510 + signature
times 510 - ($ - $$) db 0
dw 0xAA55
