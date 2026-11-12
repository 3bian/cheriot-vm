// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once
#include <cdefs.h>

__always_inline static int rdcycle()
{
	int instructions;
	__asm__ volatile("csrr %0, minstret" : "=r"(instructions));
	return instructions;
}
