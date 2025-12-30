/* Linker script for RayOS bare-metal kernel */

ENTRY(_start)

SECTIONS
{
    /* Load the kernel at 16MB physical address (avoid low-memory firmware reservations/collisions) */
    . = 16M;

    __kernel_start = .;

    /* Keep the entry point at the very beginning */
    .text : ALIGN(4K)
    {
        KEEP(*(.text._start))
        *(.text .text.*)
    }

    .rodata : ALIGN(4K)
    {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(4K)
    {
        *(.data .data.*)
    }

    .bss : ALIGN(4K)
    {
        *(.bss .bss.*)
        *(COMMON)
    }

    __kernel_end = .;

    /* Discard unnecessary sections */
    /DISCARD/ :
    {
        *(.eh_frame)
        *(.comment)
    }
}
