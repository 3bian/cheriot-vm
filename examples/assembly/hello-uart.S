/* Copyright 3bian Limited and Contributors */
/* SPDX-License-Identifier: MIT */

.option norvc
# Assemble for RV32I with no runtime, link at address zero.
# This project ships preassembled hello-uart.bin as a reproducible input.
# Example with a RISC-V toolchain:
#   riscv32-unknown-elf-as -march=rv32i -mabi=ilp32 hello-uart.S -o hello-uart.o
# A console or UART at 0x10000000 receives the ASCII byte 'A'.
.section .text
.globl _start
_start:
    lui x1, 0x10000
    addi x2, x0, 65
    sw x2, 0(x1)
    ebreak
