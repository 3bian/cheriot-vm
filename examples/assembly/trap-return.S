/* Copyright 3bian Limited and Contributors */
/* SPDX-License-Identifier: MIT */

/* Machine-mode ECALL handler: advance mepc and return with MRET.
 * At done (0x18): a0=1 (handler ran once), a1=42. */
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
