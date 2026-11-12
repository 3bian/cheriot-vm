// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub mod permissions {
    pub const GLOBAL: u16 = 1;
    pub const LOAD_GLOBAL: u16 = 1 << 1;
    pub const STORE: u16 = 1 << 2;
    pub const LOAD_MUTABLE: u16 = 1 << 3;
    pub const STORE_LOCAL: u16 = 1 << 4;
    pub const LOAD: u16 = 1 << 5;
    pub const LOAD_STORE_CAPABILITY: u16 = 1 << 6;
    pub const SYSTEM_REGISTERS: u16 = 1 << 7;
    pub const EXECUTE: u16 = 1 << 8;
    pub const UNSEAL: u16 = 1 << 9;
    pub const SEAL: u16 = 1 << 10;
    pub const USER0: u16 = 1 << 11;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capability {
    bits: u64,
    tag: bool,
}

impl Capability {
    pub const fn address(self) -> u32 {
        self.bits as u32
    }

    pub fn alignment_mask(length: u32) -> u32 {
        let root = Self::from_memory(0x7e3e_0000_0000_0000, true);
        u32::MAX << root.set_bounds(length, false).0.exponent()
    }

    pub fn bounds(self) -> (u32, u64) {
        let exponent = self.exponent();
        let bottom = ((self.bits >> 32) & 511) as i64;
        let top = ((self.bits >> 41) & 511) as i64;
        let address = i64::from(self.address());
        let region = address >> (exponent + 9);
        let address_middle = (address >> exponent) & 511;
        let base_region = region - i64::from(address_middle < bottom);
        let top_region = base_region + i64::from(top < bottom);
        let base = ((base_region * 512 + bottom) << exponent) as u32;
        let top = (((top_region * 512 + top) << exponent) as u64) & 0x1_ffff_ffff;
        (base, top)
    }

    pub fn clear_tag(mut self) -> Self {
        self.tag = false;
        self
    }

    pub fn contains(self, address: u32, length: u64) -> bool {
        let (base, top) = self.bounds();
        address >= base
            && u64::from(address)
                .checked_add(length)
                .is_some_and(|end| end <= top)
    }

    pub fn exponent(self) -> u32 {
        let exponent = ((self.bits >> 50) & 15) as u32;

        if exponent == 15 { 24 } else { exponent }
    }

    #[inline]
    pub fn revocation_bit(self, memory_base: u32) -> Option<(u32, u8)> {
        let sealing_permissions = permissions::SEAL | permissions::UNSEAL | permissions::USER0;
        let base = self.bounds().0;
        if self.permissions() & sealing_permissions != 0 || base < memory_base {
            return None;
        }
        Some(((base - memory_base) >> 6, 1 << ((base >> 3) & 7)))
    }

    pub fn filter_revoked(mut self, revoked_base_granule: bool) -> Self {
        let sealing_permissions = permissions::SEAL | permissions::UNSEAL | permissions::USER0;

        if revoked_base_granule && self.permissions() & sealing_permissions == 0 {
            self.tag = false;
        }

        self
    }

    pub const fn from_memory(bits: u64, tag: bool) -> Self {
        Self { bits, tag }
    }

    pub const fn integer(value: u32) -> Self {
        Self {
            bits: value as u64,
            tag: false,
        }
    }

    pub fn length(self) -> u32 {
        let (base, top) = self.bounds();
        top.wrapping_sub(u64::from(base)).min(u64::from(u32::MAX)) as u32
    }

    pub fn object_type(self) -> u8 {
        let compressed = ((self.bits >> 54) & 7) as u8;

        if compressed == 0 || self.permissions() & permissions::EXECUTE != 0 {
            compressed
        } else {
            compressed | 8
        }
    }

    pub fn permissions(self) -> u16 {
        use permissions::*;
        let packed = ((self.bits >> 57) & 63) as u16;
        let mut permissions = (packed >> 5) & 1;
        let low = packed & 31;
        let optional_load = ((low & 2) << 2) | ((low & 1) << 1);
        permissions |= match low {
            24..=31 => LOAD | STORE | LOAD_STORE_CAPABILITY | ((low & 4) << 2) | optional_load,
            20..=23 => LOAD | LOAD_STORE_CAPABILITY | optional_load,
            16 => STORE | LOAD_STORE_CAPABILITY,
            17..=19 => ((low & 2) << 4) | ((low & 1) << 2),
            8..=15 => EXECUTE | LOAD | LOAD_STORE_CAPABILITY | ((low & 4) << 5) | optional_load,
            _ => ((low & 4) << 9) | ((low & 2) << 9) | ((low & 1) << 9),
        };
        permissions
    }

    pub fn restrict_permissions(self, mask: u16) -> Self {
        use permissions::*;
        let mask = mask & 0xfff;
        let restricted_permissions = self.permissions() & mask;
        let has = |bits| restricted_permissions & bits == bits;
        let optional_load = ((restricted_permissions & LOAD_MUTABLE) >> 2)
            | ((restricted_permissions & LOAD_GLOBAL) >> 1);
        let compressed = if has(EXECUTE | LOAD | LOAD_STORE_CAPABILITY) {
            8 | ((restricted_permissions & SYSTEM_REGISTERS) >> 5) | optional_load
        } else if has(LOAD | STORE | LOAD_STORE_CAPABILITY) {
            24 | ((restricted_permissions & STORE_LOCAL) >> 2) | optional_load
        } else if has(LOAD | LOAD_STORE_CAPABILITY) {
            20 | optional_load
        } else if has(STORE | LOAD_STORE_CAPABILITY) {
            16
        } else if restricted_permissions & (LOAD | STORE) != 0 {
            16 | ((restricted_permissions & LOAD) >> 4) | ((restricted_permissions & STORE) >> 2)
        } else {
            ((restricted_permissions & USER0) >> 9)
                | ((restricted_permissions & SEAL) >> 9)
                | ((restricted_permissions & UNSEAL) >> 9)
        } | ((restricted_permissions & GLOBAL) << 5);
        Self {
            bits: (self.bits & !(63u64 << 57)) | (u64::from(compressed) << 57),
            tag: self.tag && (!self.sealed() || mask | GLOBAL == 0xfff),
        }
    }

    pub fn rounded_length(length: u32) -> u32 {
        let mask = Self::alignment_mask(length);
        length.wrapping_add(!mask) & mask
    }

    pub fn seal(self, authority: Self) -> Self {
        let object_type = authority.address();
        let permitted_type = if self.permissions() & permissions::EXECUTE != 0 {
            (1..=7).contains(&object_type)
        } else {
            (9..=15).contains(&object_type)
        };
        let tag = self.tag
            && !self.sealed()
            && authority.tag
            && !authority.sealed()
            && authority.permissions() & permissions::SEAL != 0
            && authority.contains(object_type, 1)
            && permitted_type;
        Self {
            bits: (self.bits & !(7u64 << 54)) | (u64::from(object_type & 7) << 54),
            tag,
        }
    }

    pub const fn sealed(self) -> bool {
        self.bits & (7u64 << 54) != 0
    }

    pub fn set_address(self, address: u32) -> Self {
        let mut result = Self {
            bits: (self.bits & !0xffff_ffff) | u64::from(address),
            tag: self.tag,
        };
        result.tag &= !self.sealed() && result.bounds() == self.bounds();
        result
    }

    pub fn set_bounds(self, length: u32, require_exact: bool) -> (Self, bool) {
        let base = u64::from(self.address());
        let top = base + u64::from(length);

        let minimum_exponent = (u32::BITS - length.leading_zeros()).saturating_sub(9);
        let exponent = (minimum_exponent..=14)
            .chain([24])
            .find(|&exponent| {
                let unit = 1u64 << exponent;
                top.div_ceil(unit) - base / unit <= 511
            })
            .unwrap_or(24);
        let unit = 1u64 << exponent;
        let exact = base.is_multiple_of(unit) && top.is_multiple_of(unit);
        let bottom = (base / unit) & 511;
        let rounded_top = top.div_ceil(unit) & 511;
        let encoded_exponent = if exponent == 24 { 15 } else { exponent };
        let bits = (self.bits & !(((1u64 << 22) - 1) << 32))
            | (u64::from(encoded_exponent) << 50)
            | (rounded_top << 41)
            | (bottom << 32);
        let tag = self.tag
            && !self.sealed()
            && self.contains(self.address(), u64::from(length))
            && (!require_exact || exact);
        (Self { bits, tag }, exact)
    }

    pub fn set_bounds_round_down(self, length: u32) -> Self {
        let address = self.address();
        let length_exponent = (32 - length.leading_zeros()).saturating_sub(9);
        let exponent = length_exponent.min(address.trailing_zeros()).min(14);
        let bottom = u64::from(address >> exponent) & 511;
        let span = if exponent < length_exponent {
            511
        } else {
            u64::from(length >> exponent)
        };
        let top = (bottom + span) & 511;
        let bits = (self.bits & !(((1u64 << 22) - 1) << 32))
            | (u64::from(exponent) << 50)
            | (top << 41)
            | (bottom << 32);
        Self {
            bits,
            tag: self.tag && !self.sealed() && self.contains(address, u64::from(length)),
        }
    }

    pub fn subset_contains(self, other: Self) -> bool {
        let (base, top) = self.bounds();
        let (other_base, other_top) = other.bounds();
        self.tag == other.tag
            && base <= other_base
            && top >= other_top
            && other.permissions() & !self.permissions() == 0
    }

    pub const fn tag(self) -> bool {
        self.tag
    }

    pub const fn to_memory(self) -> (u64, bool) {
        (self.bits, self.tag)
    }

    pub fn unseal(self, authority: Self) -> Self {
        let authority_permissions = authority.permissions();
        let tag = self.tag
            && self.sealed()
            && authority.tag
            && !authority.sealed()
            && authority_permissions & permissions::UNSEAL != 0
            && authority.contains(u32::from(self.object_type()), 1);
        let mut bits = self.bits & !(7u64 << 54);

        if authority_permissions & permissions::GLOBAL == 0 {
            bits &= !(1u64 << 62);
        }

        Self { bits, tag }
    }
}

#[cfg(test)]
#[path = "../tests/cheriot/capability.rs"]
mod tests;
