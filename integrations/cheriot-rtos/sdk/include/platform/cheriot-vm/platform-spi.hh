// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <cstddef>
#include <cstdint>
#include <utils.hh>

/** Synchronous byte transfers through the emulator's SPI interface. */
struct VirtualSpi : private utils::NoCopyNoMove
{
	/// Register blocks are obtained from MMIO capabilities, never constructed.
	VirtualSpi() = delete;

	/// Writing starts a transfer; reading returns its received byte.
	uint32_t data;
	/// Completion and missing-peripheral status of the last transfer.
	const uint32_t status;
	/// Selected peripheral, or UINT32_MAX to deselect all peripherals.
	uint32_t chipSelect;
	/// Clock mode in the range 0..3.
	uint32_t mode;
	/// Completion-interrupt enable.
	uint32_t control;
	/// Write StatusComplete to acknowledge a completed transfer.
	uint32_t acknowledge;

	/// Number of supported chip-select lines.
	static constexpr uint32_t ChipSelectCount = 4;

	/// Status-register fields.
	enum : uint32_t
	{
		StatusComplete          = 1 << 0,
		StatusMissingPeripheral = 1 << 1,
	};

	/// Control-register fields.
	enum : uint32_t
	{
		ControlInterruptEnable = 1 << 0,
	};

	/**
	 * Configure the chip select and clock mode, disabling interrupts and
	 * acknowledging completion. Invalid arguments return false without writes.
	 */
	bool init(uint32_t select = 0, uint32_t clockMode = 0) volatile
	{
		if (select >= ChipSelectCount || clockMode > 3)
		{
			return false;
		}
		control     = 0;
		acknowledge = StatusComplete;
		chipSelect  = select;
		mode        = clockMode;
		return true;
	}

	/// Enable or disable completion interrupts.
	void interrupt_enable(bool enable) volatile
	{
		control = enable ? ControlInterruptEnable : 0;
	}

	/// Clear the completion interrupt before completing it in the scheduler.
	void interrupt_acknowledge() volatile
	{
		acknowledge = StatusComplete;
	}

	/// Deselect all peripherals. Subsequent transfers report a missing device.
	void deselect() volatile
	{
		chipSelect = UINT32_MAX;
	}

	/// Transfer a byte immediately, returning false if no peripheral responded.
	bool transfer(uint8_t outgoing, uint8_t &incoming) volatile
	{
		data            = outgoing;
		uint32_t result = status;
		incoming        = static_cast<uint8_t>(data);
		return (result & (StatusComplete | StatusMissingPeripheral)) ==
		       StatusComplete;
	}
};

static_assert(sizeof(VirtualSpi) == 24);
static_assert(offsetof(VirtualSpi, acknowledge) == 20);
