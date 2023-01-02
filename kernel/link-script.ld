ENTRY(_start)

SECTIONS
{
  . = 0x00100000;

  .text :
  {
    *(.text*)
  }

  .rodata :
  {
    *(.rodata*)
  }

  .data :
  {
    *(.data*)
  }

  .bss :
  {
    __bss_start = .;

    *(.bss*)

    __bss_end = .;
  }
}
