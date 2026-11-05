// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <cstddef>
#include <cstdint>
#include <utils.hh>

/** Synchronous I2C byte commands. Serialise access across each transaction. */
struct VirtualI2c : private utils::NoCopyNoMove
{
	/// Register blocks are obtained from MMIO capabilities, never constructed.
	VirtualI2c() = delete;

	/// Seven-bit address of the selected peripheral.
	uint32_t address;
	/// Outgoing byte for a write, or received byte after a successful read.
	uint32_t data;
	/// Writing a Command value executes it immediately.
	uint32_t command;
	/// Acknowledgement, bus state and command-completion flags.
	const uint32_t status;
	/// Command-completion interrupt enable.
	uint32_t control;
	/// Write StatusComplete to acknowledge the completion interrupt.
	uint32_t acknowledge;

	/// Commands accepted by the virtual controller.
	enum Command : uint32_t
	{
		CommandWrite   = 1,
		CommandRead    = 2,
		CommandStop    = 3,
		CommandRestart = 4,
	};

	/// Status-register fields.
	enum : uint32_t
	{
		StatusAcknowledged = 1 << 0,
		StatusComplete     = 1 << 2,
		StatusError        = 1 << 3,
	};

	/// Control-register fields.
	enum : uint32_t
	{
		ControlInterruptEnable = 1 << 0,
	};

	/// Disable interrupts, terminate any transaction and clear completion.
	void init() volatile
	{
		control     = 0;
		command     = CommandStop;
		acknowledge = StatusComplete;
	}

	/// Select a seven-bit target. Return false without writing for other
	/// values.
	bool address_select(uint8_t target) volatile
	{
		if (target > 0x7f)
		{
			return false;
		}
		address = target;
		return true;
	}

	/// Enable or disable command-completion interrupts.
	void interrupt_enable(bool enable) volatile
	{
		control = enable ? ControlInterruptEnable : 0;
	}

	/// Clear completion before completing the interrupt in the scheduler.
	void interrupt_acknowledge() volatile
	{
		acknowledge = StatusComplete;
	}

	/// End the current transaction and release the selected peripheral.
	void stop() volatile
	{
		command = CommandStop;
	}

	/// Begin a repeated START without ending the selected transaction.
	void restart() volatile
	{
		command = CommandRestart;
	}

	/// Write one byte, starting a transaction if idle. Return false on NACK.
	bool byte_write(uint8_t byte) volatile
	{
		data    = byte;
		command = CommandWrite;
		return (status & (StatusAcknowledged | StatusError)) ==
		       StatusAcknowledged;
	}

	/// Read one byte. On NACK, return false without changing the output byte.
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

	/**
	 * Write then read a target, with a repeated START between nonempty phases.
	 * Buffers must be accessible for their supplied lengths. Invalid addresses
	 * return false without touching the bus. Once selected, every path issues
	 * STOP, including a NACK; failed transactions may have transferred a
	 * prefix.
	 */
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
