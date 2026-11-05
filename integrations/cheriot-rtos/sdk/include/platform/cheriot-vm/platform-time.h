// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <cdefs.h>
#include <compartment-macros.h>
#include <stdint.h>

/** Read the stable 64-bit CLINT counter using a read-only MMIO capability. */
__if_c(static) inline uint64_t platform_monotonic_time_read()
{
	const volatile uint32_t *timer = MMIO_CAPABILITY_WITH_PERMISSIONS(
	  uint32_t, clint, true, false, false, false, false);
	uint32_t high;
	uint32_t low;
	do
	{
		high = timer[0xbffc / 4];
		low  = timer[0xbff8 / 4];
	} while (high != timer[0xbffc / 4]);
	return ((uint64_t)high << 32) | low;
}
