// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <cstddef>
#include <cstdint>
#include <utils.hh>

struct VirtualStorage : private utils::NoCopyNoMove
{
	VirtualStorage() = delete;

	uint32_t sector;
	uint32_t wordIndex;
	uint32_t data;
	uint32_t command;
	uint32_t status;
	uint32_t control;

	static constexpr size_t SectorBytes = 512;

	enum Command : uint32_t
	{
		CommandRead  = 1,
		CommandWrite = 2,
	};

	enum : uint32_t
	{
		StatusComplete = 1 << 0,
		StatusError    = 1 << 1,
		StatusPresent  = 1 << 2,
	};

	enum : uint32_t
	{
		ControlInterruptEnable = 1 << 0,
	};

	void init() volatile
	{
		control = 0;
		interrupt_acknowledge();
	}

	bool is_present() volatile
	{
		return (status & StatusPresent) != 0;
	}

	void interrupt_enable(bool enable) volatile
	{
		control = enable ? ControlInterruptEnable : 0;
	}

	void interrupt_acknowledge() volatile
	{
		status = StatusComplete | StatusError;
	}

	bool sector_read(uint32_t number, uint8_t *bytes, size_t length) volatile
	{
		if (length != SectorBytes)
		{
			return false;
		}
		sector  = number;
		command = CommandRead;
		if ((status & (StatusComplete | StatusError | StatusPresent)) !=
		    (StatusComplete | StatusPresent))
		{
			return false;
		}
		for (size_t index = 0; index < SectorBytes / sizeof(uint32_t); ++index)
		{
			wordIndex     = index;
			uint32_t word = data;
			for (size_t byte = 0; byte < sizeof(uint32_t); ++byte)
			{
				bytes[index * sizeof(uint32_t) + byte] =
				  static_cast<uint8_t>(word >> (byte * 8));
			}
		}
		return true;
	}

	bool
	sector_write(uint32_t number, const uint8_t *bytes, size_t length) volatile
	{
		if (length != SectorBytes)
		{
			return false;
		}
		sector = number;
		for (size_t index = 0; index < SectorBytes / sizeof(uint32_t); ++index)
		{
			uint32_t word = 0;
			for (size_t byte = 0; byte < sizeof(uint32_t); ++byte)
			{
				word |= uint32_t{bytes[index * sizeof(uint32_t) + byte]}
				        << (byte * 8);
			}
			wordIndex = index;
			data      = word;
		}
		command = CommandWrite;
		return (status & (StatusComplete | StatusError | StatusPresent)) ==
		       (StatusComplete | StatusPresent);
	}
};

static_assert(sizeof(VirtualStorage) == 24);
static_assert(offsetof(VirtualStorage, control) == 20);
