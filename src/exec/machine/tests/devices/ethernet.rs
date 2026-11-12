// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::devices::ethernet::{Ethernet, NetworkBackend};
use crate::exec::machine::devices::{Device, InterruptLine};

#[test]
fn ethernet_empty_send_full_queues_and_disconnection() {
    let mut e = Ethernet::new(NetworkBackend::InMemory);
    e.write(16, 4, 1).unwrap();
    assert_eq!(e.read(4, 4).unwrap(), 4);
    assert!(e.take_transmitted().is_none());

    for i in 0..9 {
        e.write(8, 4, 1).unwrap();
        e.write(12, 4, i).unwrap();
        e.write(16, 4, 1).unwrap();
    }

    for i in 0..8 {
        assert_eq!(e.take_transmitted().unwrap(), [i]);
    }

    assert!(e.take_transmitted().is_none());
    let mut disconnected = Ethernet::new(NetworkBackend::Disconnected);
    disconnected.set_link(true);
    assert_eq!(disconnected.read(0, 4).unwrap(), 0);
    assert!(disconnected.inject_frame(b"x").is_err());
}

#[test]
fn ethernet_level_interrupt_cannot_be_cleared_while_queued() {
    let mut ethernet = Ethernet::new(NetworkBackend::InMemory);
    ethernet.write(32, 4, 2).unwrap();
    ethernet.inject_frame(b"abcde").unwrap();
    ethernet.write(4, 4, u32::MAX).unwrap();
    assert_eq!(ethernet.read(4, 4).unwrap() & 2, 2);
    assert_eq!(ethernet.interrupt(), Some(InterruptLine::MachineExternal));
    assert_eq!(ethernet.read(24, 4).unwrap(), 0x64636261);
    assert_eq!(ethernet.read(24, 4).unwrap(), b'e' as u32);
    assert_eq!(ethernet.read(24, 4).unwrap(), 0);
    ethernet.write(28, 4, 1).unwrap();
    assert_eq!(ethernet.interrupt(), None);
}
