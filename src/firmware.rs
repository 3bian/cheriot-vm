// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::bus::Bus;
use crate::error::EmulatorError;
use crate::processor::ProcessorProfile;
use loadable_segment::LoadableSegment;

mod loadable_segment;

pub fn elf_symbol_address(bytes: &[u8], name: &str) -> Result<u32, EmulatorError> {
    use elf::{endian::LittleEndian, ElfBytes};
    let file = ElfBytes::<LittleEndian>::minimal_parse(bytes)
        .map_err(|error| firmware_error(&error.to_string()))?;
    let (symbols, names) = file
        .symbol_table()
        .map_err(|error| firmware_error(&error.to_string()))?
        .ok_or_else(|| firmware_error("completion requires an ELF symbol table"))?;
    let mut found = None;
    for symbol in symbols
        .iter()
        .filter(|symbol| names.get(symbol.st_name as usize).ok() == Some(name))
    {
        let address = u32::try_from(symbol.st_value)
            .map_err(|_| firmware_error("completion symbol exceeds the address space"))?;
        if symbol.st_shndx == 0 || found.replace(address).is_some() {
            return Err(firmware_error("completion symbol must be uniquely defined"));
        }
    }
    found.ok_or_else(|| firmware_error(&format!("completion requires a {name} symbol")))
}

pub fn load_elf(
    bus: &mut Bus,
    bytes: &[u8],
    profile: ProcessorProfile,
) -> Result<u32, EmulatorError> {
    load_elf_with_flags(bus, bytes, profile, 0x9, 0)
}

pub fn load_cheriot1_elf(bus: &mut Bus, bytes: &[u8]) -> Result<u32, EmulatorError> {
    load_elf_with_flags(
        bus,
        bytes,
        ProcessorProfile {
            compressed: true,
            control_registers: true,
            embedded: true,
            multiply_divide: true,
        },
        0x70009,
        0x70009,
    )
}

pub fn load_raw(bus: &mut Bus, address: u32, bytes: &[u8]) -> Result<(), EmulatorError> {
    if bytes.is_empty() {
        return Err(firmware_error("raw firmware must not be empty"));
    }

    bus.install_firmware(address, bytes)
}

pub(crate) fn validate_raw_entry(
    bus: &Bus,
    address: u32,
    bytes: &[u8],
    alignment: u32,
) -> Result<(), EmulatorError> {
    if !address.is_multiple_of(alignment) || bytes.len() < alignment as usize {
        return Err(firmware_error(&format!(
            "raw firmware needs a {alignment}-byte-aligned entry and at least {alignment} bytes"
        )));
    }
    if !bus
        .memory()
        .iter()
        .any(|memory| memory.executable && memory.contains(address, bytes.len()))
    {
        return Err(firmware_error("raw firmware must fit in executable memory"));
    }
    Ok(())
}

fn firmware_error(message: &str) -> EmulatorError {
    EmulatorError::Firmware(message.into())
}

fn load_elf_with_flags(
    bus: &mut Bus,
    bytes: &[u8],
    profile: ProcessorProfile,
    allowed_flags: u32,
    required_flags: u32,
) -> Result<u32, EmulatorError> {
    use elf::{abi, endian::LittleEndian, file::Class, ElfBytes};
    let file = ElfBytes::<LittleEndian>::minimal_parse(bytes)
        .map_err(|error| firmware_error(&format!("invalid ELF: {error}")))?;
    let header = file.ehdr;

    if header.class != Class::ELF32
        || header.e_type != abi::ET_EXEC
        || header.e_machine != abi::EM_RISCV
        || header.version != 1
    {
        return Err(firmware_error(
            "requires little-endian RISC-V ELF32 ET_EXEC",
        ));
    }

    let entry = u32::try_from(header.e_entry)
        .map_err(|_| firmware_error("entry exceeds RV32 address space"))?;
    let flags = header.e_flags;

    if flags & !allowed_flags != 0 || flags & required_flags != required_flags {
        return Err(firmware_error(
            "ELF ABI flags are unsupported or disagree with the selected execution mode",
        ));
    }

    if flags & 1 != 0 && !profile.compressed {
        return Err(firmware_error(
            "ELF uses compressed instructions but the processor has C disabled",
        ));
    }

    if (flags & 8 != 0) != profile.embedded {
        return Err(firmware_error(
            "ELF RV32E ABI flag disagrees with selected processor",
        ));
    }

    let headers = file
        .segments()
        .ok_or_else(|| firmware_error("missing program headers"))?;

    if headers.is_empty() || headers.len() > 1024 || header.e_ehsize != 52 {
        return Err(firmware_error("invalid ELF header or program-header count"));
    }

    let mut segments: Vec<LoadableSegment> = Vec::new();

    for segment in headers.iter() {
        if matches!(
            segment.p_type,
            abi::PT_INTERP | abi::PT_DYNAMIC | abi::PT_TLS
        ) {
            return Err(firmware_error(
                "dynamic linking, interpreters and TLS boot setup are unsupported",
            ));
        }

        if segment.p_type != abi::PT_LOAD {
            continue;
        }

        if segment.p_align > 1
            && (!segment.p_align.is_power_of_two()
                || segment.p_vaddr % segment.p_align != segment.p_offset % segment.p_align)
        {
            return Err(firmware_error("ELF segment alignment is invalid"));
        }

        if segment.p_paddr != 0 && segment.p_paddr != segment.p_vaddr {
            return Err(firmware_error("different ELF physical and virtual addresses require an unsupported relocation/boot protocol"));
        }

        if segment.p_flags & !7 != 0 {
            return Err(firmware_error("unsupported segment permission bits"));
        }

        let address = segment.p_vaddr as u32;
        let file_offset = segment.p_offset as usize;
        let file_size = segment.p_filesz as usize;
        let memory_size = segment.p_memsz as usize;
        let executable = segment.p_flags & abi::PF_X != 0;

        if memory_size == 0 && file_size == 0 {
            continue;
        }

        if memory_size < file_size || memory_size > 64 * 1024 * 1024 || memory_size == 0 {
            return Err(firmware_error("invalid or oversized ELF loadable segment"));
        }

        if address as u64 + memory_size as u64 > u32::MAX as u64 + 1 {
            return Err(firmware_error(
                "ELF segment extends outside 32-bit address space",
            ));
        }

        slice_at(bytes, file_offset, file_size)?;

        if !bus.memory.iter().any(|memory| {
            memory.contains(address, memory_size)
                && (!executable || memory.executable)
                && (segment.p_flags & abi::PF_W == 0
                    || memory.kind == crate::memory::MemoryKind::Ram)
        }) {
            return Err(firmware_error(
                "ELF segment is not fully backed by configured RAM or ROM",
            ));
        }

        let start = address as u64;
        let end = start + memory_size as u64;

        for prior in &segments {
            let other_start = prior.address as u64;
            let other_end = other_start + prior.memory_size as u64;

            if start < other_end && other_start < end {
                return Err(firmware_error(
                    "overlapping ELF loadable segments are unsupported",
                ));
            }
        }

        segments.push(LoadableSegment {
            address,
            file_offset,
            file_size,
            memory_size,
            executable,
        });
    }

    let executable_entry = segments.iter().any(|segment| {
        segment.executable
            && entry as u64 >= segment.address as u64
            && u64::from(entry) + u64::from(profile.minimum_alignment())
                <= segment.address as u64 + segment.memory_size as u64
    });

    if !executable_entry {
        return Err(firmware_error(
            "ELF entry point must be in an executable loadable segment",
        ));
    }

    if entry % profile.minimum_alignment() != 0 {
        return Err(firmware_error(
            "ELF entry point is not aligned for this processor",
        ));
    }

    for segment in segments {
        let mut image = vec![0_u8; segment.memory_size];
        image[..segment.file_size].copy_from_slice(slice_at(
            bytes,
            segment.file_offset,
            segment.file_size,
        )?);
        bus.install_firmware(segment.address, &image)?;
    }

    Ok(entry)
}

fn slice_at(bytes: &[u8], position: usize, length: usize) -> Result<&[u8], EmulatorError> {
    bytes
        .get(
            position
                ..position
                    .checked_add(length)
                    .ok_or_else(|| firmware_error("ELF offset overflow"))?,
        )
        .ok_or_else(|| firmware_error("truncated ELF header or segment"))
}

#[cfg(test)]
#[path = "../tests/firmware/tests.rs"]
mod tests;
