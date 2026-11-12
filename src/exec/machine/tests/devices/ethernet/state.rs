// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn in_memory_frames_and_interrupts() {
    let mut ethernet = Ethernet::new(NetworkBackend::InMemory);
    ethernet.write(32, 4, 3).unwrap();
    ethernet.write(8, 4, 3).unwrap();
    ethernet.write(12, 4, 0x00636261).unwrap();
    ethernet.write(16, 4, 1).unwrap();
    assert_eq!(ethernet.take_transmitted(), Some(b"abc".to_vec()));
    assert_eq!(ethernet.interrupt(), Some(InterruptLine::MachineExternal));
    ethernet.inject_frame(b"xyz").unwrap();
    assert_eq!(ethernet.read(20, 4).unwrap(), 3);
    assert_eq!(ethernet.read(24, 4).unwrap() & 0xffffff, 0x7a7978);
    ethernet.write(28, 4, 1).unwrap();
    assert_eq!(ethernet.pending_frames(), 0);
}

#[test]
fn queue_limit_and_link_down() {
    let mut ethernet = Ethernet::new(NetworkBackend::InMemory);

    for _ in 0..8 {
        ethernet.inject_frame(b"a").unwrap();
    }

    assert!(ethernet.inject_frame(b"b").is_err());
    ethernet.set_link(false);
    ethernet.write(8, 4, 1).unwrap();
    ethernet.write(12, 4, 3).unwrap();
    ethernet.write(16, 4, 1).unwrap();
    assert_eq!(ethernet.read(4, 4).unwrap() & 4, 4);
}
