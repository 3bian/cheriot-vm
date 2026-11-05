// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <compartment.h>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <utils.hh>

/**
 * RTOS PLIC interface for the emulator's fixed-priority interrupt controller.
 * Source zero is reserved; lower enabled source IDs take priority.
 */
template<size_t MaxIntrID, typename SourceID, typename Priority>
class VirtualPlic : private utils::NoCopyNoMove
{
	static_assert(MaxIntrID < 32);

	/// Register layout of the 16-byte virtual controller.
	struct Registers
	{
		/// Asserted sources that are not currently in service.
		const uint32_t pending;
		/// Bit mask of enabled sources; bit zero is ignored.
		uint32_t enabled;
		/// Read to claim a source, write its ID to complete it.
		uint32_t claimComplete;
		/// Bit mask of sources claimed and not yet completed.
		const uint32_t inService;
	};
	static_assert(sizeof(Registers) == 16);
	static_assert(offsetof(Registers, claimComplete) == 8);

	/// Capability to the virtual controller selected by the board.
	volatile Registers *registers;

	/// Return whether a source is described by this board and is nonzero.
	static bool is_valid(SourceID source)
	{
		return source > 0 && source <= MaxIntrID;
	}

	public:
	/// Initialise the controller with all sources disabled.
	VirtualPlic()
	{
		registers          = MMIO_CAPABILITY(Registers, plic);
		registers->enabled = 0;
	}

	/// Enable a source. Invalid source IDs have no effect.
	void interrupt_enable(SourceID source)
	{
		if (is_valid(source))
		{
			registers->enabled = registers->enabled | (uint32_t{1} << source);
		}
	}

	/// Disable a source. Invalid source IDs have no effect.
	void interrupt_disable(SourceID source)
	{
		if (is_valid(source))
		{
			registers->enabled = registers->enabled & ~(uint32_t{1} << source);
		}
	}

	/// RTOS compatibility hook: priority is fixed by source ID, not
	/// programmable.
	void priority_set(SourceID, Priority) {}

	/// Claim the lowest enabled pending source, or return an empty optional.
	std::optional<SourceID> interrupt_claim()
	{
		uint32_t source = registers->claimComplete;
		return source == 0 ? std::nullopt
		                   : std::optional{static_cast<SourceID>(source)};
	}

	/**
	 * Complete a previously claimed source. Clear its device condition first.
	 * Invalid source IDs are ignored; completing an unclaimed source faults.
	 */
	void interrupt_complete(SourceID source)
	{
		if (is_valid(source))
		{
			registers->claimComplete = source;
		}
	}
};

/** RTOS platform interface for the virtual PLIC. */
template<size_t MaxIntrID, typename SourceID, typename Priority>
using Plic = VirtualPlic<MaxIntrID, SourceID, Priority>;
