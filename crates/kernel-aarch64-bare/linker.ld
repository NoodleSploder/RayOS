/* Linker script for RayOS aarch64 bare-metal kernel */

ENTRY(_start)

SECTIONS
{
    /* QEMU virt RAM starts at 0x4000_0000; keep the kernel in RAM. */
    . = 0x40200000;

    __kernel_start = .;

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

    /DISCARD/ :
    {
        *(.eh_frame)
        *(.comment)
    }
}
