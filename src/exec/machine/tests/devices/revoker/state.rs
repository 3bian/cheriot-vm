// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::Revoker;
use crate::exec::machine::bus::Bus;
use crate::exec::machine::cheriot::{Capability, Computer};
use crate::exec::machine::devices::interrupt_controller::InterruptController;
use crate::exec::machine::devices::{Device, InterruptLine};
use crate::exec::machine::memory::{MemoryKind, MemoryRegion, Tagging};

const RAM: u32 = Computer::REVOCABLE_MEMORY_BASE;
const BITMAP: u32 = Computer::REVOCATION_BITMAP_BASE;
const REVOKER: u32 = 0x1000_4000;
const CONTROLLER: u32 = 0x0c00_0000;
const BASE: u32 = REVOKER;
const TOP: u32 = REVOKER + 0x04;
const CONTROL: u32 = REVOKER + 0x08;
const EPOCH: u32 = REVOKER + 0x0c;
const STATUS: u32 = REVOKER + 0x10;
const REQUESTED: u32 = REVOKER + 0x14;

fn pointer_to(base: u32) -> Capability {
    let root = Capability::from_memory(0x7e3e_0000_0000_0000, true);
    let capability = root.set_address(base).set_bounds(64, true).0;
    assert!(capability.tag());
    assert_eq!(capability.bounds().0, base);
    capability
}

fn bus() -> Bus {
    let mut bus = Bus::new();
    for (name, base, size) in [("ram", RAM, 0x4000), ("shadow", BITMAP, 0x100)] {
        bus.add_memory(
            MemoryRegion::new(name.into(), base, size, MemoryKind::Ram, Tagging::Cheriot).unwrap(),
        )
        .unwrap();
    }
    bus.add_device(
        "interrupts".into(),
        CONTROLLER,
        0x100,
        Box::new(InterruptController::new()),
    )
    .unwrap();
    bus.add_device(
        "revoker0".into(),
        REVOKER,
        0x100,
        Box::new(Revoker::new(RAM, BITMAP)),
    )
    .unwrap();
    bus.set_interrupt_source("revoker0", Some(7)).unwrap();
    bus
}

fn mark(bus: &mut Bus, base: u32, freed: bool) {
    let address = BITMAP + ((base - RAM) >> 6);
    let bit = 1 << ((base >> 3) & 7);
    let byte = bus.read(address, 1).unwrap();
    let byte = if freed { byte | bit } else { byte & !bit };
    bus.write(address, 1, byte).unwrap();
}

fn start(bus: &mut Bus, base: u32, top: u32) {
    bus.write(BASE, 4, base).unwrap();
    bus.write(TOP, 4, top).unwrap();
    bus.write(CONTROL, 4, 0).unwrap();
    bus.write(CONTROL, 4, 1).unwrap();
}

fn finish(bus: &mut Bus) -> u32 {
    let mut ticks = 0;
    while bus.read(EPOCH, 4).unwrap() & 1 == 1 {
        bus.tick_devices(1);
        ticks += 1;
        assert!(ticks < 10_000, "the sweep never finished");
    }
    ticks
}

fn tag(bus: &Bus, address: u32) -> bool {
    bus.read_capability(address).unwrap().tag()
}

#[test]
fn a_sweep_clears_the_tag_of_every_capability_to_freed_memory_and_no_other() {
    let mut bus = bus();
    let (freed, live) = (RAM + 0x1000, RAM + 0x1040);
    mark(&mut bus, freed, true);
    let stale = [RAM + 0x2000, RAM + 0x2ff8, RAM + 0x2800];
    let kept = [RAM + 0x2008, RAM + 0x2ff0, RAM + 0x2808];
    for address in stale {
        bus.write_capability(address, pointer_to(freed)).unwrap();
    }
    for address in kept {
        bus.write_capability(address, pointer_to(live)).unwrap();
    }
    let sealing_root = Capability::from_memory(0x4e3e_0000_0000_0000, true);
    assert!(sealing_root.revocation_bit(0).is_none());
    bus.write_capability(RAM + 0x2010, sealing_root).unwrap();
    bus.write_capability(RAM + 0x3008, pointer_to(freed))
        .unwrap();
    bus.write_capability(RAM + 0x1ff8, pointer_to(freed))
        .unwrap();
    let bytes_before = bus.read_capability(stale[0]).unwrap().to_memory().0;

    start(&mut bus, RAM + 0x2000, RAM + 0x2ff8);
    finish(&mut bus);

    for address in stale {
        assert!(!tag(&bus, address), "{address:#x} kept a stale tag");
    }
    for address in kept {
        assert!(tag(&bus, address), "{address:#x} lost a live tag");
    }
    assert!(tag(&bus, RAM + 0x2010));
    assert!(tag(&bus, RAM + 0x3008));
    assert!(tag(&bus, RAM + 0x1ff8));
    assert_eq!(
        bus.read_capability(stale[0]).unwrap().to_memory().0,
        bytes_before
    );
}

#[test]
fn a_swept_pointer_stays_dead_after_the_memory_is_reused() {
    let mut swept = bus();
    let mut unswept = bus();
    let (freed, holder) = (RAM + 0x1000, RAM + 0x2000);
    for bus in [&mut swept, &mut unswept] {
        bus.write_capability(holder, pointer_to(freed)).unwrap();
        mark(bus, freed, true);
    }
    start(&mut swept, RAM, RAM + 0x3ff8);
    finish(&mut swept);
    for bus in [&mut swept, &mut unswept] {
        mark(bus, freed, false);
    }
    assert!(!tag(&swept, holder));
    assert!(
        tag(&unswept, holder),
        "without a sweep the old pointer is usable again: the hazard the revoker removes"
    );
}

#[test]
fn the_range_is_granules_from_base_to_the_one_holding_top() {
    let mut bus = bus();
    let freed = RAM + 0x1000;
    mark(&mut bus, freed, true);
    for address in [0x2000, 0x2008, 0x2010, 0x2018, 0x2020] {
        bus.write_capability(RAM + address, pointer_to(freed))
            .unwrap();
    }
    start(&mut bus, RAM + 0x200c, RAM + 0x201b);
    finish(&mut bus);
    assert!(tag(&bus, RAM + 0x2000));
    assert!(!tag(&bus, RAM + 0x2008));
    assert!(!tag(&bus, RAM + 0x2010));
    assert!(!tag(&bus, RAM + 0x2018));
    assert!(tag(&bus, RAM + 0x2020));

    start(&mut bus, RAM + 0x2000, RAM + 0x1000);
    finish(&mut bus);
    start(&mut bus, RAM + 0x3ff0, RAM + 0x8000);
    finish(&mut bus);
    start(&mut bus, 0, 0x400);
    finish(&mut bus);
    assert!(tag(&bus, RAM + 0x2000));
    assert_eq!(bus.read(EPOCH, 4).unwrap(), 8);
}

#[test]
fn the_epoch_is_odd_during_a_sweep_and_counts_the_finished_ones() {
    let mut bus = bus();
    assert_eq!(bus.read(CONTROL, 4).unwrap(), Revoker::IDENTITY);
    assert_eq!(bus.read(EPOCH, 4).unwrap(), 0);

    let granules = 4 * Revoker::GRANULES_PER_TICK as u32;
    bus.write(BASE, 4, RAM).unwrap();
    bus.write(TOP, 4, RAM + granules * 8 - 8).unwrap();
    assert_eq!(bus.read(BASE, 4).unwrap(), RAM);
    assert_eq!(bus.read(TOP, 4).unwrap(), RAM + granules * 8 - 8);
    assert_eq!(
        bus.read(EPOCH, 4).unwrap(),
        0,
        "setting the range starts nothing"
    );

    bus.write(CONTROL, 4, 1).unwrap();
    assert_eq!(bus.read(EPOCH, 4).unwrap(), 1);
    bus.tick_devices(1);
    assert_eq!(bus.read(EPOCH, 4).unwrap(), 1);
    bus.write(CONTROL, 4, 1).unwrap();
    assert_eq!(finish(&mut bus), 3);
    assert_eq!(bus.read(EPOCH, 4).unwrap(), 2);

    start(&mut bus, RAM, RAM);
    assert_eq!(bus.read(EPOCH, 4).unwrap(), 3);
    finish(&mut bus);
    assert_eq!(bus.read(EPOCH, 4).unwrap(), 4);

    bus.write(EPOCH, 4, 99).unwrap();
    assert_eq!(bus.read(EPOCH, 4).unwrap(), 4);
    assert!(bus.read(EPOCH, 1).is_err());
    assert!(bus.read(REVOKER + 0x18, 4).is_err());
}

#[test]
fn a_tick_visits_a_bounded_number_of_granules() {
    let mut bus = bus();
    let freed = RAM + 0x1000;
    mark(&mut bus, freed, true);
    let per_tick = Revoker::GRANULES_PER_TICK as u32 * 8;
    let (first, second) = (RAM + 0x2000, RAM + 0x2000 + per_tick);
    bus.write_capability(first, pointer_to(freed)).unwrap();
    bus.write_capability(second, pointer_to(freed)).unwrap();
    start(&mut bus, first, second);
    bus.tick_devices(1);
    assert!(!tag(&bus, first));
    assert!(tag(&bus, second), "the second tick's work was done early");
    bus.write_capability(first, pointer_to(freed)).unwrap();
    finish(&mut bus);
    assert!(!tag(&bus, second));
    assert!(tag(&bus, first));
}

#[test]
fn the_status_is_polled_when_no_interrupt_is_requested() {
    let mut bus = bus();
    assert_eq!(bus.read(STATUS, 4).unwrap(), 0);
    start(&mut bus, RAM, RAM);
    assert_eq!(bus.read(STATUS, 4).unwrap(), 0);
    finish(&mut bus);
    assert_eq!(bus.read(STATUS, 4).unwrap(), 1);
    bus.refresh_interrupts();
    assert!(!bus.interrupt_pending(InterruptLine::MachineExternal));
    bus.tick_devices(5);
    assert_eq!(bus.read(STATUS, 4).unwrap(), 1);
    bus.write(STATUS, 4, 1).unwrap();
    assert_eq!(bus.read(STATUS, 4).unwrap(), 0);
}

#[test]
fn a_requested_interrupt_is_one_pulse_that_waits_to_be_claimed() {
    let mut bus = bus();
    bus.write(CONTROLLER + 4, 4, 1 << 7).unwrap();
    bus.write(REQUESTED, 4, 1).unwrap();
    assert_eq!(bus.read(REQUESTED, 4).unwrap(), 1);
    start(&mut bus, RAM, RAM);
    bus.refresh_interrupts();
    assert!(!bus.interrupt_pending(InterruptLine::MachineExternal));

    finish(&mut bus);
    bus.refresh_interrupts();
    assert!(bus.interrupt_pending(InterruptLine::MachineExternal));
    assert_eq!(bus.read(STATUS, 4).unwrap(), 0);

    assert!(
        bus.device::<Revoker>("revoker0")
            .unwrap()
            .interrupt()
            .is_none()
    );
    bus.tick_devices(50);
    bus.refresh_interrupts();
    assert!(bus.interrupt_pending(InterruptLine::MachineExternal));

    assert_eq!(bus.read(CONTROLLER + 8, 4).unwrap(), 7);
    bus.write(CONTROLLER + 8, 4, 7).unwrap();
    bus.refresh_interrupts();
    assert!(!bus.interrupt_pending(InterruptLine::MachineExternal));
    bus.tick_devices(50);
    bus.refresh_interrupts();
    assert!(!bus.interrupt_pending(InterruptLine::MachineExternal));
    assert_eq!(bus.read(CONTROLLER + 8, 4).unwrap(), 0);

    assert_eq!(bus.read(REQUESTED, 4).unwrap(), 1);
    start(&mut bus, RAM, RAM);
    finish(&mut bus);
    bus.refresh_interrupts();
    assert_eq!(bus.read(CONTROLLER + 8, 4).unwrap(), 7);
    bus.write(CONTROLLER + 8, 4, 7).unwrap();
}

#[test]
fn back_to_back_sweeps_raise_one_interrupt_each() {
    let mut bus = bus();
    bus.write(CONTROLLER + 4, 4, 1 << 7).unwrap();
    bus.write(REQUESTED, 4, 1).unwrap();
    let mut interrupts = 0;
    for _ in 0..4 {
        start(&mut bus, RAM, RAM);
        finish(&mut bus);
        bus.refresh_interrupts();
        while bus.read(CONTROLLER + 8, 4).unwrap() == 7 {
            bus.write(CONTROLLER + 8, 4, 7).unwrap();
            bus.refresh_interrupts();
            interrupts += 1;
        }
    }
    assert_eq!(interrupts, 4);
    assert_eq!(bus.read(EPOCH, 4).unwrap(), 8);
}

#[test]
fn a_request_made_after_a_sweep_finished_still_interrupts() {
    let mut bus = bus();
    bus.write(CONTROLLER + 4, 4, 1 << 7).unwrap();
    start(&mut bus, RAM, RAM);
    finish(&mut bus);
    bus.refresh_interrupts();
    assert!(!bus.interrupt_pending(InterruptLine::MachineExternal));
    bus.write(REQUESTED, 4, 1).unwrap();
    bus.refresh_interrupts();
    assert!(bus.interrupt_pending(InterruptLine::MachineExternal));
    assert_eq!(bus.read(CONTROLLER + 8, 4).unwrap(), 7);
}

#[test]
fn memory_is_reached_only_through_the_port_and_only_on_a_tick() {
    let mut bus = bus();
    let freed = RAM + 0x1000;
    mark(&mut bus, freed, true);
    bus.write_capability(RAM + 0x2000, pointer_to(freed))
        .unwrap();
    start(&mut bus, RAM + 0x2000, RAM + 0x2000);
    assert!(tag(&bus, RAM + 0x2000));
    let mut alone = Revoker::new(RAM, BITMAP);
    alone.write(0x08, 4, 1).unwrap();
    alone.tick(1_000);
    assert_eq!(alone.read(0x0c, 4).unwrap(), 1);
    bus.tick_devices(1);
    assert!(!tag(&bus, RAM + 0x2000));
}
