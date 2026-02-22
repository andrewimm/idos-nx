ENTRY(_flat_entry)

FATDRV_LOAD_ADDR = 0x400000;

SECTIONS
{
  . = FATDRV_LOAD_ADDR;

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

  . = ALIGN(0x1000);

  .data :
  {
    *(.data*)
  }

  .bss :
  {
    *(.bss*)
  }
}
