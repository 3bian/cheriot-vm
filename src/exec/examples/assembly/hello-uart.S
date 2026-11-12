/* Copyright 3Bian Limited and Contributors */
/* SPDX-License-Identifier: MIT */

.option norvc
.section .text
.globl _start
_start:
    lui x1, 0x10000
    addi x2, x0, 65
    sw x2, 0(x1)
    ebreak
