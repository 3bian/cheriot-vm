// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <cstddef>
#include <cstdint>
#include <utils.hh>

struct VirtualGpio : private utils::NoCopyNoMove
{
	VirtualGpio() = delete;

	const uint32_t pins;
	uint32_t output;
	uint32_t direction;
	uint32_t interruptMask;
	uint32_t pending;

	static constexpr uint32_t PinCount = 32;

	void init() volatile
	{
		interruptMask = 0;
		pending       = UINT32_MAX;
		direction     = 0;
	}

	uint32_t read() volatile
	{
		return pins;
	}

	void write(uint32_t values) volatile
	{
		output = values;
	}

	void output_enable(uint32_t mask) volatile
	{
		direction = mask;
	}

	void interrupt_enable(uint32_t mask) volatile
	{
		interruptMask = mask;
	}

	void interrupt_acknowledge(uint32_t mask) volatile
	{
		pending = mask;
	}

	bool output_set(uint32_t pin, bool value) volatile
	{
		if (pin >= PinCount)
		{
			return false;
		}
		uint32_t bit = uint32_t{1} << pin;
		output       = value ? output | bit : output & ~bit;
		return true;
	}
};

static_assert(sizeof(VirtualGpio) == 20);
static_assert(offsetof(VirtualGpio, pending) == 16);
