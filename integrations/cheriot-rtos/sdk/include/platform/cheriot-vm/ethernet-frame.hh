// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#pragma once

#include <array>
#include <cheri.hh>
#include <cstdint>

/** An owned receive frame whose read-only view follows it when moved. */
class EthernetFrame
{
	/// Storage is zeroed so rounded capability bounds never expose stale bytes.
	alignas(8) std::array<uint8_t, 1520> bytes{};
	friend class VirtualEthernet;

	/// Let the Ethernet driver fill this frame before publishing its view.
	uint8_t *writable_buffer()
	{
		return bytes.data();
	}

	/// Bound the read-only view to this object after construction or a move.
	void rebind()
	{
		CHERI::Capability<const uint8_t> capability{bytes.data()};
		capability.bounds().set_inexact(length);
		capability.permissions() &=
		  CHERI::PermissionSet{CHERI::Permission::Load};
		buffer = capability.get();
	}

	/// Create storage for a length already validated by VirtualEthernet.
	explicit EthernetFrame(uint16_t size) : length(size)
	{
		rebind();
	}

	public:
	/// Received byte count, excluding the Ethernet FCS.
	uint16_t length;
	/// Bounded, read-only view valid until this object is moved or destroyed.
	const uint8_t *buffer;

	/// Frames transfer ownership by moving, not copying.
	EthernetFrame(const EthernetFrame &) = delete;
	/// Frames cannot be copy-assigned.
	EthernetFrame &operator=(const EthernetFrame &) = delete;

	/// Move the bytes and rebind the view to the destination storage.
	EthernetFrame(EthernetFrame &&other)
	  : bytes(other.bytes), length(other.length)
	{
		rebind();
	}

	/// Replace this frame with the moved bytes and rebind its view.
	EthernetFrame &operator=(EthernetFrame &&other)
	{
		bytes  = other.bytes;
		length = other.length;
		rebind();
		return *this;
	}
};
