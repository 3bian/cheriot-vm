// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <cstddef>
#include <cstdint>
#include <utils.hh>

/** Volatile sector media. Byte packing permits unaligned caller buffers. */
struct VirtualStorage : private utils::NoCopyNoMove
{
	/// Register blocks are obtained from MMIO capabilities, never constructed.
	VirtualStorage() = delete;

	/// Sector selected for the next command.
	uint32_t sector;
	/// Word offset within the 512-byte transfer buffer, in the range 0..127.
	uint32_t wordIndex;
	/// Read or write the selected transfer-buffer word.
	uint32_t data;
	/// Writing a Command value transfers a sector immediately.
	uint32_t command;
	/// Completion, error and media-present flags. Completion/error are W1C.
	uint32_t status;
	/// Completion and error interrupt enable.
	uint32_t control;

	/// Size of a sector and the controller's transfer buffer.
	static constexpr size_t SectorBytes = 512;

	/// Commands accepted by the virtual controller.
	enum Command : uint32_t
	{
		CommandRead  = 1,
		CommandWrite = 2,
	};

	/// Status-register fields.
	enum : uint32_t
	{
		StatusComplete = 1 << 0,
		StatusError    = 1 << 1,
		StatusPresent  = 1 << 2,
	};

	/// Control-register fields.
	enum : uint32_t
	{
		ControlInterruptEnable = 1 << 0,
	};

	/// Disable interrupts and acknowledge any preceding command.
	void init() volatile
	{
		control = 0;
		interrupt_acknowledge();
	}

	/// Return whether the virtual device has media attached.
	bool is_present() volatile
	{
		return (status & StatusPresent) != 0;
	}

	/// Enable or disable interrupts for completed and failed commands.
	void interrupt_enable(bool enable) volatile
	{
		control = enable ? ControlInterruptEnable : 0;
	}

	/// Clear completion and error before completing the scheduler interrupt.
	void interrupt_acknowledge() volatile
	{
		status = StatusComplete | StatusError;
	}

	/**
	 * Read exactly one sector into an accessible byte buffer. Return false for
	 * a wrong length, missing media or out-of-range sector without changing it.
	 */
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

	/**
	 * Write exactly one sector from an accessible byte buffer. Return false for
	 * a wrong length, missing or read-only media, or an out-of-range sector.
	 */
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
