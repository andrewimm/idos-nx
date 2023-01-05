ENTRY(_start)

SECTIONS
{
  . = 0x2000;

  .entry :
  {
    *(.entry)
  }

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
    *(.bss*)
  }
}
