// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <cstddef>
#include <cstdint>
#include <utils.hh>

struct VirtualI2c : private utils::NoCopyNoMove
{
	VirtualI2c() = delete;

	uint32_t address;
	uint32_t data;
	uint32_t command;
	const uint32_t status;
	uint32_t control;
	uint32_t acknowledge;

	enum Command : uint32_t
	{
		CommandWrite   = 1,
		CommandRead    = 2,
		CommandStop    = 3,
		CommandRestart = 4,
	};

	enum : uint32_t
	{
		StatusAcknowledged = 1 << 0,
		StatusComplete     = 1 << 2,
		StatusError        = 1 << 3,
	};

	enum : uint32_t
	{
		ControlInterruptEnable = 1 << 0,
	};

	void init() volatile
	{
		control     = 0;
		command     = CommandStop;
		acknowledge = StatusComplete;
	}

	bool address_select(uint8_t target) volatile
	{
		if (target > 0x7f)
		{
			return false;
		}
		address = target;
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

	void stop() volatile
	{
		command = CommandStop;
	}

	void restart() volatile
	{
		command = CommandRestart;
	}

	bool byte_write(uint8_t byte) volatile
	{
		data    = byte;
		command = CommandWrite;
		return (status & (StatusAcknowledged | StatusError)) ==
		       StatusAcknowledged;
	}

	bool byte_read(uint8_t &byte) volatile
	{
		command = CommandRead;
		if ((status & (StatusAcknowledged | StatusError)) != StatusAcknowledged)
		{
			return false;
		}
		byte = static_cast<uint8_t>(data);
		return true;
	}

	bool write_read(uint8_t        target,
	                const uint8_t *outgoing,
	                size_t         outgoingLength,
	                uint8_t       *incoming,
	                size_t         incomingLength) volatile
	{
		if (!address_select(target))
		{
			return false;
		}
		bool success = true;
		for (size_t index = 0; index < outgoingLength && success; ++index)
		{
			success = byte_write(outgoing[index]);
		}
		if (success && outgoingLength != 0 && incomingLength != 0)
		{
			restart();
		}
		for (size_t index = 0; index < incomingLength && success; ++index)
		{
			success = byte_read(incoming[index]);
		}
		stop();
		return success;
	}
};

static_assert(sizeof(VirtualI2c) == 24);
static_assert(offsetof(VirtualI2c, acknowledge) == 20);
