// Copyright 3Bian Limited and Contributors
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

DECLARE_AND_DEFINE_INTERRUPT_CAPABILITY(ethernetReceive,
                                        InterruptName::EthernetInterrupt,
                                        true,
                                        true);

struct VirtualEthernetRegisters : private utils::NoCopyNoMove
{
	VirtualEthernetRegisters() = delete;

	const uint32_t link;
	uint32_t status;
	uint32_t transmitLength;
	uint32_t transmitData;
	uint32_t transmitSend;
	const uint32_t receiveLength;
	const uint32_t receiveData;
	uint32_t receivePop;
	uint32_t interruptMask;

	enum : uint32_t
	{
		StatusTransmitComplete = 1 << 0,
		StatusReceiveReady     = 1 << 1,
		StatusTransferError    = 1 << 2,
		StatusTransmitError    = StatusTransferError,
	};
};

static_assert(sizeof(VirtualEthernetRegisters) == 36);
static_assert(offsetof(VirtualEthernetRegisters, interruptMask) == 32);

class VirtualEthernet : private utils::NoCopyNoMove
{
	volatile VirtualEthernetRegisters *registers;
	const volatile uint32_t *receiveInterrupt = nullptr;
	uint32_t completedInterrupt = 0;
	std::array<uint8_t, 6> macAddress = mac_address_default();

	public:
	VirtualEthernet()
	{
		registers = MMIO_CAPABILITY(VirtualEthernetRegisters, ethernet);
		receiveInterrupt =
		  interrupt_futex_get(STATIC_SEALED_VALUE(ethernetReceive));
		registers->interruptMask = VirtualEthernetRegisters::StatusReceiveReady;
	}

	static constexpr std::array<uint8_t, 6> mac_address_default()
	{
		return {2, 0, 0, 0, 0, 2};
	}

	static constexpr bool has_unique_mac_address()
	{
		return false;
	}

	void mac_address_set(std::array<uint8_t, 6> address = mac_address_default())
	{
		macAddress = address;
	}

	std::array<uint8_t, 6> mac_address_get() const
	{
		return macAddress;
	}

	bool phy_link_status() const
	{
		return registers->link != 0;
	}

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
		registers->status = VirtualEthernetRegisters::StatusTransmitComplete;
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
		return (registers->status &
		        VirtualEthernetRegisters::StatusTransmitComplete) != 0;
	}

	uint32_t receive_interrupt_value() const
	{
		return receiveInterrupt ? *receiveInterrupt : 0;
	}

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

using EthernetDevice = VirtualEthernet;
