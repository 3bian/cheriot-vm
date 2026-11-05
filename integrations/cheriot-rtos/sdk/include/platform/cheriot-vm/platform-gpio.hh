// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <cstddef>
#include <cstdint>
#include <utils.hh>

/**
 * GPIO pins with rising-input interrupts. Serialise read-modify-write access.
 */
struct VirtualGpio : private utils::NoCopyNoMove
{
	/// Register blocks are obtained from MMIO capabilities, never constructed.
	VirtualGpio() = delete;

	/// Input samples combined with the latch values of output-enabled pins.
	const uint32_t pins;
	/// Output latches; bits only drive pins enabled in `direction`.
	uint32_t output;
	/// One bit per pin: one selects output, zero selects input.
	uint32_t direction;
	/// One bit per pin enabling its rising-input interrupt.
	uint32_t interruptMask;
	/// Pending rising edges. Write ones to acknowledge the corresponding pins.
	uint32_t pending;

	/// Number of logical pins in the register interface.
	static constexpr uint32_t PinCount = 32;

	/// Disable interrupts, acknowledge pending edges and select input mode.
	void init() volatile
	{
		interruptMask = 0;
		pending       = UINT32_MAX;
		direction     = 0;
	}

	/// Read the current values of all logical pins.
	uint32_t read() volatile
	{
		return pins;
	}

	/// Replace all output latch values without changing pin directions.
	void write(uint32_t values) volatile
	{
		output = values;
	}

	/// Enable output on the pins in `mask`, making all other pins inputs.
	void output_enable(uint32_t mask) volatile
	{
		direction = mask;
	}

	/// Enable rising-input interrupts for precisely the pins in `mask`.
	void interrupt_enable(uint32_t mask) volatile
	{
		interruptMask = mask;
	}

	/// Acknowledge the pending rising edges selected by `mask`.
	void interrupt_acknowledge(uint32_t mask) volatile
	{
		pending = mask;
	}

	/// Set one output latch. Return false without writing for an invalid pin.
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
