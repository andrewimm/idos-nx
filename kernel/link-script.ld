ENTRY(_start)

KERNEL_VADDR_OFFSET = 0xC0000000;
KERNEL_OFFSET = 0xC0100000;

SECTIONS
{
  . = KERNEL_OFFSET;

  . += SIZEOF_HEADERS;
  . = ALIGN(0x1000);

  __kernel_start = . - KERNEL_VADDR_OFFSET;

  .text : AT(ADDR(.text) - KERNEL_OFFSET)
  {
    *(.text*)
  }

  .rodata : AT(ADDR(.rodata) - KERNEL_OFFSET)
  {
    *(.rodata*)
  }

  . = ALIGN(0x1000);

  .data : AT(ADDR(.data) - KERNEL_OFFSET)
  {
    *(.data*)
  }

  .bss : AT(ADDR(.bss) - KERNEL_OFFSET)
  {
    __bss_start = . - KERNEL_VADDR_OFFSET;

    *(.bss*)

    __bss_end = . - KERNEL_VADDR_OFFSET;

    . = ALIGN(0x1000);

    __input_buffers = . - KERNEL_VADDR_OFFSET;

    . += 0x1000;

    __stack_start = . - KERNEL_VADDR_OFFSET;

    . += 0x4000;

    __stack_end = . - KERNEL_VADDR_OFFSET;
  }

  __kernel_end = . - KERNEL_VADDR_OFFSET;
}
