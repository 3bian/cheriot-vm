/* Copyright 3bian Limited and Contributors */
/* SPDX-License-Identifier: MIT */

/* Guest checks all deterministic teaching peripherals in the public RV32IMC profile.
 * a0=0 on success, a0=1 on failure. Run with --registers.
 */
.option norvc
.section .text
.globl _start
_start:
  li a0, 1
  li t0, 0x10001000 /* SPI loopback */
  sw zero, 8(t0)
  li t1, 0x95
  sw t1, 0(t0)
  lw t2, 0(t0)
  bne t1, t2, done
  li t0, 0x10002000 /* I2C EEPROM: write value at offset 16 */
  li t1, 0x50
  sw t1, 0(t0)
  li t1, 16
  sw t1, 4(t0)
  li t2, 1
  sw t2, 8(t0)
  li t1, 0xab
  sw t1, 4(t0)
  sw t2, 8(t0)
  li t1, 3
  sw t1, 8(t0) /* STOP */
  li t1, 16
  sw t1, 4(t0)
  sw t2, 8(t0)
  li t1, 2
  sw t1, 8(t0)
  lw t1, 4(t0)
  li t2, 0xab
  bne t1, t2, done
  li t0, 0x10003000 /* GPIO output bit 0 */
  li t1, 1
  sw t1, 8(t0)
  sw t1, 4(t0)
  lw t2, 0(t0)
  bne t1, t2, done
  li t0, 0x02004000 /* timer64 compare */
  li t1, 2
  sw t1, 0(t0)
  sw zero, 4(t0)
  csrr t2, mip
  andi t2, t2, 128
  beqz t2, done
  li t0, 0x30000000 /* storage round trip */
  li t1, 0x12345678
  sw t1, 8(t0)
  li t2, 2
  sw t2, 12(t0)
  sw zero, 8(t0)
  li t2, 1
  sw t2, 12(t0)
  lw t2, 8(t0)
  bne t1, t2, done
  li t0, 0x20000000 /* Ethernet: capture abc */
  li t1, 3
  sw t1, 8(t0)
  li t1, 0x00636261
  sw t1, 12(t0)
  li t1, 1
  sw t1, 16(t0)
  lw t2, 4(t0)
  bne t1, t2, done
  li a0, 0
done:
  ebreak
