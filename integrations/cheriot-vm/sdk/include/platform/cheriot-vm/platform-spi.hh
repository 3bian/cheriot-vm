// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <cstddef>
#include <cstdint>
#include <utils.hh>

struct VirtualSpi : private utils::NoCopyNoMove
{
	VirtualSpi() = delete;

	uint32_t data;
	const uint32_t status;
	uint32_t chipSelect;
	uint32_t mode;
	uint32_t control;
	uint32_t acknowledge;

	static constexpr uint32_t ChipSelectCount = 4;

	enum : uint32_t
	{
		StatusComplete          = 1 << 0,
		StatusMissingPeripheral = 1 << 1,
	};

	enum : uint32_t
	{
		ControlInterruptEnable = 1 << 0,
	};

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

	void interrupt_enable(bool enable) volatile
	{
		control = enable ? ControlInterruptEnable : 0;
	}

	void interrupt_acknowledge() volatile
	{
		acknowledge = StatusComplete;
	}

	void deselect() volatile
	{
		chipSelect = UINT32_MAX;
	}

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
