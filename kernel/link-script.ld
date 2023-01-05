ENTRY(_start)

SECTIONS
{
  . = 0x00100000;

  . += SIZEOF_HEADERS;
  . = ALIGN(0x1000);

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

  __kernel_end = .;
}
