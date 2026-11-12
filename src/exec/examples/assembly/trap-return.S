/* Copyright 3Bian Limited and Contributors */
/* SPDX-License-Identifier: MIT */

.option norvc
.option norelax
.section .text
.globl _start
_start:
  la t0, handler
  csrw mtvec, t0
  li a0, 0
  ecall
  li a1, 42
done:
  j done
.balign 64
handler:
  csrr t0, mcause
  li t1, 11
  bne t0, t1, failed
  csrr t0, mepc
  addi t0, t0, 4
  csrw mepc, t0
  addi a0, a0, 1
  mret
failed:
  li a0, -1
  j done
