// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <array>
#include <cheri.hh>
#include <cstdint>

class EthernetFrame
{
	alignas(8) std::array<uint8_t, 1520> bytes{};
	friend class VirtualEthernet;

	uint8_t *writable_buffer()
	{
		return bytes.data();
	}

	void rebind()
	{
		CHERI::Capability<const uint8_t> capability{bytes.data()};
		capability.bounds().set_inexact(length);
		capability.permissions() &=
		  CHERI::PermissionSet{CHERI::Permission::Load};
		buffer = capability.get();
	}

	explicit EthernetFrame(uint16_t size) : length(size)
	{
		rebind();
	}

	public:
	uint16_t length;
	const uint8_t *buffer;

	EthernetFrame(const EthernetFrame &) = delete;
	EthernetFrame &operator=(const EthernetFrame &) = delete;

	EthernetFrame(EthernetFrame &&other)
	  : bytes(other.bytes), length(other.length)
	{
		rebind();
	}

	EthernetFrame &operator=(EthernetFrame &&other)
	{
		bytes  = other.bytes;
		length = other.length;
		rebind();
		return *this;
	}
};
