ENTRY(_start)

SECTIONS
{
  . = 0x00100000;

  . += SIZEOF_HEADERS;
  . = ALIGN(0x1000);

  __kernel_start = .;

  .text :
  {
    *(.text*)
  }

  .rodata :
  {
    *(.rodata*)
  }

  . = ALIGN(0x1000);

  .data :
  {
    *(.data*)
  }

  .bss :
  {
    __bss_start = .;

    *(.bss*)

    __bss_end = .;

    . = ALIGN(0x1000);

    __stack_start = .;

    . += 0x1000;
    
    __stack_end = .;
  }

  __kernel_end = .;
}
