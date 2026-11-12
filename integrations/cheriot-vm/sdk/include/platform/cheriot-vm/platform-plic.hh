// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <compartment.h>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <utils.hh>

template<size_t MaxIntrID, typename SourceID, typename Priority>
class VirtualPlic : private utils::NoCopyNoMove
{
	static_assert(MaxIntrID < 32);

	struct Registers
	{
		const uint32_t pending;
		uint32_t enabled;
		uint32_t claimComplete;
		const uint32_t inService;
	};
	static_assert(sizeof(Registers) == 16);
	static_assert(offsetof(Registers, claimComplete) == 8);

	volatile Registers *registers;

	static bool is_valid(SourceID source)
	{
		return source > 0 && source <= MaxIntrID;
	}

	public:
	VirtualPlic()
	{
		registers          = MMIO_CAPABILITY(Registers, plic);
		registers->enabled = 0;
	}

	void interrupt_enable(SourceID source)
	{
		if (is_valid(source))
		{
			registers->enabled = registers->enabled | (uint32_t{1} << source);
		}
	}

	void interrupt_disable(SourceID source)
	{
		if (is_valid(source))
		{
			registers->enabled = registers->enabled & ~(uint32_t{1} << source);
		}
	}

	void priority_set(SourceID, Priority) {}

	std::optional<SourceID> interrupt_claim()
	{
		uint32_t source = registers->claimComplete;
		return source == 0 ? std::nullopt
		                   : std::optional{static_cast<SourceID>(source)};
	}

	void interrupt_complete(SourceID source)
	{
		if (is_valid(source))
		{
			registers->claimComplete = source;
		}
	}
};

template<size_t MaxIntrID, typename SourceID, typename Priority>
using Plic = VirtualPlic<MaxIntrID, SourceID, Priority>;
