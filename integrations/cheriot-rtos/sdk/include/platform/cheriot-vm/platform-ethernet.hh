// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <array>
#include <cheri.hh>
#include <compartment.h>
#include <cstddef>
#include <cstdint>
#include <errno.h>
#include <ethernet-frame.hh>
#include <futex.h>
#include <interrupt.h>
#include <optional>
#include <platform/concepts/ethernet.hh>
#include <utils.hh>

/// Authority to wait for and complete Ethernet receive interrupts.
DECLARE_AND_DEFINE_INTERRUPT_CAPABILITY(ethernetReceive,
                                        InterruptName::EthernetInterrupt,
                                        true,
                                        true);

/**
 * Register layout of the virtual Ethernet frame queues.
 */
struct VirtualEthernetRegisters : private utils::NoCopyNoMove
{
	/// Register blocks are obtained from MMIO capabilities, never constructed.
	VirtualEthernetRegisters() = delete;

	/// Link availability; nonzero while the backend is up.
	const uint32_t link;
	/// TX completion, RX availability and TX error flags. TX flags are W1C.
	uint32_t status;
	/// Length of the frame being staged, excluding FCS.
	uint32_t transmitLength;
	/// Append up to four little-endian bytes to the staged transmit frame.
	uint32_t transmitData;
	/// Write one to submit the staged frame.
	uint32_t transmitSend;
	/// Length of the next frame, or zero if the queue is empty.
	const uint32_t receiveLength;
	/// Read up to four little-endian bytes from the current receive frame.
	const uint32_t receiveData;
	/// Write one to discard the current receive frame and advance the queue.
	uint32_t receivePop;
	/// Enable interrupts selected by the status-bit mask.
	uint32_t interruptMask;

	/// Status and interrupt-mask fields.
	enum : uint32_t
	{
		StatusTransmitComplete = 1 << 0,
		StatusReceiveReady     = 1 << 1,
		StatusTransmitError    = 1 << 2,
	};
};

static_assert(sizeof(VirtualEthernetRegisters) == 36);
static_assert(offsetof(VirtualEthernetRegisters, interruptMask) == 32);

/**
 * RTOS Ethernet adapter. Callers must serialise access to the device.
 */
class VirtualEthernet : private utils::NoCopyNoMove
{
	/// Capability to the board-selected frame-queue registers.
	volatile VirtualEthernetRegisters *registers;
	/// Scheduler-owned counter incremented when the RX interrupt is claimed.
	const volatile uint32_t *receiveInterrupt = nullptr;
	/// Last counter value completed; never complete an unclaimed source.
	uint32_t completedInterrupt = 0;
	/// Software source address; the controller performs no MAC filtering.
	std::array<uint8_t, 6> macAddress = mac_address_default();

	public:
	/// Attach to the board device and enable receive-queue interrupts.
	VirtualEthernet()
	{
		registers = MMIO_CAPABILITY(VirtualEthernetRegisters, ethernet);
		receiveInterrupt =
		  interrupt_futex_get(STATIC_SEALED_VALUE(ethernetReceive));
		registers->interruptMask = VirtualEthernetRegisters::StatusReceiveReady;
	}

	/**
	 * Return the default locally administered address; change it for each
	 * guest.
	 */
	static constexpr std::array<uint8_t, 6> mac_address_default()
	{
		return {2, 0, 0, 0, 0, 2};
	}

	/**
	 * The default software MAC is shared by all instances.
	 */
	static constexpr bool has_unique_mac_address()
	{
		return false;
	}

	/**
	 * Change the software MAC address without programming a hardware filter.

	 */
	void mac_address_set(std::array<uint8_t, 6> address = mac_address_default())
	{
		macAddress = address;
	}

	/**
	 * Return the configured software MAC address.
	 */
	std::array<uint8_t, 6> mac_address_get() const
	{
		return macAddress;
	}

	/**
	 * Return whether the backend reports an available link.
	 */
	bool phy_link_status() const
	{
		return registers->link != 0;
	}

	/**
	 * Copy and dequeue one frame. Return empty for an empty queue or invalid
	 * length.
	 */
	std::optional<EthernetFrame> receive_frame()
	{
		uint32_t length = registers->receiveLength;
		if (length == 0)
		{
			return std::nullopt;
		}
		if (length < 14 || length > 1518)
		{
			registers->receivePop = 1;
			return std::nullopt;
		}
		EthernetFrame frame{static_cast<uint16_t>(length)};
		for (size_t i = 0; i < length; i += 4)
		{
			uint32_t word = registers->receiveData;
			for (size_t j = 0; j < 4 && i + j < length; ++j)
			{
				frame.writable_buffer()[i + j] =
				  static_cast<uint8_t>(word >> (j * 8));
			}
		}
		registers->receivePop = 1;
		return frame;
	}

	/**
	 * Send a 14..1518-byte frame after validating an immutable snapshot. The
	 * callback receives a bounded read-only capability without Global
	 * permission. Return false for an invalid length, link failure, rejected
	 * snapshot or failed submission. The caller must supply a readable buffer
	 * of `length` bytes. The snapshot needs 1520 bytes of stack and is never
	 * retained.
	 */
	template<typename Check>
	bool send_frame(const uint8_t *buffer, uint16_t length, Check &&check)
	{
		if (length < 14 || length > 1518 || !phy_link_status())
		{
			return false;
		}
		alignas(8) std::array<uint8_t, 1520> snapshot{};
		for (size_t i = 0; i < length; ++i)
		{
			snapshot[i] = buffer[i];
		}
		CHERI::Capability<const uint8_t> checked{snapshot.data()};
		checked.bounds().set_inexact(length);
		checked.permissions() &= CHERI::PermissionSet{CHERI::Permission::Load};
		if (!check(checked.get(), length))
		{
			return false;
		}
		constexpr uint32_t TransmitStatus =
		  VirtualEthernetRegisters::StatusTransmitComplete |
		  VirtualEthernetRegisters::StatusTransmitError;
		registers->status         = TransmitStatus;
		registers->transmitLength = length;
		for (size_t i = 0; i < length; i += 4)
		{
			uint32_t word = 0;
			for (size_t j = 0; j < 4 && i + j < length; ++j)
			{
				word |= uint32_t{snapshot[i + j]} << (j * 8);
			}
			registers->transmitData = word;
		}
		registers->transmitSend = 1;
		return (registers->status & TransmitStatus) ==
		       VirtualEthernetRegisters::StatusTransmitComplete;
	}

	/**
	 * Snapshot the scheduler counter before draining received frames.
	 */
	uint32_t receive_interrupt_value() const
	{
		return receiveInterrupt ? *receiveInterrupt : 0;
	}

	/**
	 * Complete a newly claimed RX interrupt, then wait for the counter to
	 * change. Drain received frames first. Return the scheduler error or
	 * timeout code.
	 */
	int receive_interrupt_complete(Timeout *timeout, uint32_t lastValue)
	{
		if (!receiveInterrupt)
		{
			return -EPERM;
		}
		uint32_t value = *receiveInterrupt;
		if (value != completedInterrupt)
		{
			int result =
			  interrupt_complete(STATIC_SEALED_VALUE(ethernetReceive));
			if (result != 0)
			{
				return result;
			}
			completedInterrupt = value;
		}
		return futex_timed_wait(timeout, receiveInterrupt, lastValue);
	}
};

static_assert(EthernetAdaptor<VirtualEthernet>);

/** RTOS platform interface for the virtual Ethernet controller. */
using EthernetDevice = VirtualEthernet;
