/* Copyright 3bian Limited and Contributors */
/* SPDX-License-Identifier: MIT */

/* Exact companion for compressed.bin: use halfwords to preserve this tiny image. */
.section .text
.globl _start
_start:
  .2byte 0x0085 /* c.addi x1,1 */
  .2byte 0x0085 /* c.addi x1,1 */
  .4byte 0x00100073 /* ebreak */
