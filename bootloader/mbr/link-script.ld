ENTRY(_start)

SECTIONS
{
  . = 0x7c00;

  /* simulate the FAT header of a disk image */
  .fat :
  {
    BYTE(0xeb)
    BYTE(0x3c)
    BYTE(0x90)
  }

  . = 0x7c3e;


  .text ALIGN(1) :
  {
    *(.boot*)
    *(.text*)
  }

  .rodata :
  {
    *(.rodata*)
  }

  .data :
  {
    *(.data*)
    *(.got*)
  }

  . = 0x7c00 + 510;
  .magic :
  {
    SHORT(0xaa55)
  }

  /DISCARD/ :
  {
    *(.eh_*)
  }
}
