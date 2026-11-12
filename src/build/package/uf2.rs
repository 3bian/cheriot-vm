// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::{io::Write, num::NonZeroU16, ops::Range};

const START0: u32 = 0x0a32_4655;
const START1: u32 = 0x9e5d_5157;
const END: u32 = 0x0ab1_6f30;
const FAMILY: u32 = 0x2000;

#[derive(Clone, Debug)]
pub struct FlashRegion {
    pub address: u32,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct FlashImage {
    pub regions: Vec<FlashRegion>,
}
#[derive(Clone, Debug)]
pub struct Uf2Layout {
    pub family_id: Option<u32>,
    pub payload_size: NonZeroU16,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Uf2Report {
    pub block_count: u32,
    pub payload_bytes: u64,
    pub address_range: Range<u32>,
}
#[derive(Debug)]
pub enum PackageError {
    Invalid(&'static str),
    Io(std::io::Error),
}
impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(s) => write!(f, "invalid UF2: {s}"),
            Self::Io(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for PackageError {}
impl From<std::io::Error> for PackageError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
fn layout_check(layout: &Uf2Layout) -> Result<usize, PackageError> {
    let n = usize::from(layout.payload_size.get());
    if n > 476 || n % 4 != 0 {
        return Err(PackageError::Invalid(
            "payload size must be a multiple of four between 4 and 476",
        ));
    }
    Ok(n)
}
fn put(block: &mut [u8], offset: usize, value: u32) {
    block[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn get(block: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        block[offset],
        block[offset + 1],
        block[offset + 2],
        block[offset + 3],
    ])
}
pub fn encode_uf2(
    image: &FlashImage,
    layout: &Uf2Layout,
    output: &mut dyn Write,
) -> Result<Uf2Report, PackageError> {
    let payload = layout_check(layout)?;
    if image.regions.is_empty() {
        return Err(PackageError::Invalid("image has no regions"));
    }
    let mut regions: Vec<_> = image.regions.iter().collect();
    regions.sort_by_key(|r| r.address);
    let mut end = 0u32;
    let mut count = 0usize;
    let mut total = 0u64;
    for (i, r) in regions.iter().enumerate() {
        if r.address % 4 != 0 || r.bytes.is_empty() || r.bytes.len() % 4 != 0 {
            return Err(PackageError::Invalid(
                "regions must be nonempty and four-byte aligned",
            ));
        }
        let len =
            u32::try_from(r.bytes.len()).map_err(|_| PackageError::Invalid("region too large"))?;
        let next = r
            .address
            .checked_add(len)
            .ok_or(PackageError::Invalid("region address overflow"))?;
        if i > 0 && r.address < end {
            return Err(PackageError::Invalid("regions overlap"));
        }
        end = next;
        count = count
            .checked_add(r.bytes.len().div_ceil(payload))
            .ok_or(PackageError::Invalid("too many blocks"))?;
        total = total
            .checked_add(u64::from(len))
            .ok_or(PackageError::Invalid("image too large"))?;
    }
    let blocks = u32::try_from(count).map_err(|_| PackageError::Invalid("too many blocks"))?;
    let mut index = 0u32;
    for r in &regions {
        for (chunk_index, chunk) in r.bytes.chunks(payload).enumerate() {
            let offset = chunk_index
                .checked_mul(payload)
                .and_then(|v| u32::try_from(v).ok())
                .ok_or(PackageError::Invalid("address overflow"))?;
            let addr = r
                .address
                .checked_add(offset)
                .ok_or(PackageError::Invalid("address overflow"))?;
            let mut block = [0u8; 512];
            put(&mut block, 0, START0);
            put(&mut block, 4, START1);
            put(
                &mut block,
                8,
                if layout.family_id.is_some() {
                    FAMILY
                } else {
                    0
                },
            );
            put(&mut block, 12, addr);
            put(
                &mut block,
                16,
                u32::try_from(chunk.len())
                    .map_err(|_| PackageError::Invalid("payload too large"))?,
            );
            put(&mut block, 20, index);
            put(&mut block, 24, blocks);
            put(&mut block, 28, layout.family_id.unwrap_or(0));
            block[32..32 + chunk.len()].copy_from_slice(chunk);
            put(&mut block, 508, END);
            output.write_all(&block)?;
            index += 1;
        }
    }
    Ok(Uf2Report {
        block_count: blocks,
        payload_bytes: total,
        address_range: regions[0].address..end,
    })
}
pub fn validate_uf2(input: &[u8], layout: &Uf2Layout) -> Result<Uf2Report, PackageError> {
    let max = layout_check(layout)?;
    if input.is_empty() || !input.len().is_multiple_of(512) {
        return Err(PackageError::Invalid("file must contain whole blocks"));
    }
    let count =
        u32::try_from(input.len() / 512).map_err(|_| PackageError::Invalid("too many blocks"))?;
    let mut seen = std::collections::BTreeSet::new();
    let mut ranges = Vec::new();
    let mut total = 0u64;
    for block in input.as_chunks::<512>().0.iter() {
        if get(block, 0) != START0 || get(block, 4) != START1 || get(block, 508) != END {
            return Err(PackageError::Invalid("bad block magic"));
        }
        let flag = get(block, 8);
        let expected = if layout.family_id.is_some() {
            FAMILY
        } else {
            0
        };
        if flag != expected || get(block, 28) != layout.family_id.unwrap_or(0) {
            return Err(PackageError::Invalid("unsupported flags or wrong family"));
        }
        if get(block, 24) != count || get(block, 20) >= count || !seen.insert(get(block, 20)) {
            return Err(PackageError::Invalid("invalid or duplicate block number"));
        }
        let addr = get(block, 12);
        let len = get(block, 16);
        if !addr.is_multiple_of(4) || len == 0 || !len.is_multiple_of(4) || len as usize > max {
            return Err(PackageError::Invalid(
                "invalid payload or address alignment",
            ));
        }
        let end = addr
            .checked_add(len)
            .ok_or(PackageError::Invalid("address overflow"))?;
        ranges.push((addr, end));
        total += u64::from(len);
    }
    ranges.sort_unstable();
    for w in ranges.windows(2) {
        if w[1].0 < w[0].1 {
            return Err(PackageError::Invalid("overlapping block addresses"));
        }
    }
    let first = ranges.first().ok_or(PackageError::Invalid("no blocks"))?.0;
    let last = ranges.last().ok_or(PackageError::Invalid("no blocks"))?.1;
    Ok(Uf2Report {
        block_count: count,
        payload_bytes: total,
        address_range: first..last,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn layout() -> Uf2Layout {
        Uf2Layout {
            family_id: Some(0x12345678),
            payload_size: NonZeroU16::new(256).unwrap(),
        }
    }
    #[test]
    fn known_block_and_round_trip_preserve_address_and_payload() {
        let image = FlashImage {
            regions: vec![FlashRegion {
                address: 0x1000,
                bytes: (0..=255).cycle().take(260).collect(),
            }],
        };
        let mut out = vec![];
        let report = encode_uf2(&image, &layout(), &mut out).unwrap();
        assert_eq!(report.block_count, 2);
        assert_eq!(
            &out[0..8],
            &[0x55, 0x46, 0x32, 0x0a, 0x57, 0x51, 0x5d, 0x9e]
        );
        assert_eq!(&out[12..16], &[0x00, 0x10, 0x00, 0x00]);
        assert_eq!(&out[28..32], &[0x78, 0x56, 0x34, 0x12]);
        assert_eq!(&out[508..512], &[0x30, 0x6f, 0xb1, 0x0a]);
        assert_eq!(&out[32..288], &image.regions[0].bytes[..256]);
        assert_eq!(&out[544..548], &image.regions[0].bytes[256..]);
        assert_eq!(validate_uf2(&out, &layout()).unwrap(), report);
    }
    #[test]
    fn matches_makerdiary_converter_for_aligned_full_blocks() {
        let input = include_bytes!("../tests/fixtures/makerdiary/input.bin");
        let reference = include_bytes!("../tests/fixtures/makerdiary/reference.uf2");
        let layout = Uf2Layout {
            family_id: Some(0xADA5_2840),
            payload_size: NonZeroU16::new(256).unwrap(),
        };
        let image = FlashImage {
            regions: vec![FlashRegion {
                address: 0x1000,
                bytes: input.to_vec(),
            }],
        };
        let mut output = Vec::new();
        let report = encode_uf2(&image, &layout, &mut output).unwrap();
        assert_eq!(&output, reference);
        assert_eq!(validate_uf2(reference, &layout).unwrap(), report);
    }
    #[test]
    fn accepts_makerdiary_padded_last_block_without_extending_our_image() {
        let source = include_bytes!("../tests/fixtures/makerdiary/input.bin");
        let reference = include_bytes!("../tests/fixtures/makerdiary/reference-short.uf2");
        let layout = Uf2Layout {
            family_id: Some(0xADA5_2840),
            payload_size: NonZeroU16::new(256).unwrap(),
        };
        let image = FlashImage {
            regions: vec![FlashRegion {
                address: 0x1000,
                bytes: source[..260].to_vec(),
            }],
        };
        let mut encoded = Vec::new();
        let report = encode_uf2(&image, &layout, &mut encoded).unwrap();
        let oracle_report = validate_uf2(reference, &layout).unwrap();
        assert_eq!(oracle_report.payload_bytes, 512);
        assert_eq!(report.payload_bytes, 260);
        assert_eq!(&encoded[..512], &reference[..512]);
        assert_eq!(&encoded[512 + 32..512 + 36], &reference[512 + 32..512 + 36]);
        assert_eq!(validate_uf2(&encoded, &layout).unwrap(), report);
    }
    #[test]
    fn rejects_overlap_overflow_corruption_and_duplicates() {
        let mut image = FlashImage {
            regions: vec![
                FlashRegion {
                    address: 0x1000,
                    bytes: vec![1; 256],
                },
                FlashRegion {
                    address: 0x1000,
                    bytes: vec![2; 4],
                },
            ],
        };
        assert!(encode_uf2(&image, &layout(), &mut vec![]).is_err());
        image.regions[1].address = 0xffff_fffc;
        assert!(encode_uf2(&image, &layout(), &mut vec![]).is_err());
        image.regions[1].address = 0x2000;
        let mut out = vec![];
        encode_uf2(&image, &layout(), &mut out).unwrap();
        out[512 + 20..512 + 24].copy_from_slice(&0u32.to_le_bytes());
        assert!(validate_uf2(&out, &layout()).is_err());
        out[0] = 0;
        assert!(validate_uf2(&out, &layout()).is_err());
    }
}
