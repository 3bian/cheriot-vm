// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once
#include <cdefs.h>

/** Return the low 32 bits of retired instructions for RTOS profiling. */
__always_inline static int rdcycle()
{
	int instructions;
	__asm__ volatile("csrr %0, minstret" : "=r"(instructions));
	return instructions;
}
