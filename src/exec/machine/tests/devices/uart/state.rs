// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn independent_receive_and_transmit() {
    let mut uart = Uart::new();
    uart.inject_receive(b"A").unwrap();
    uart.write(8, 4, 1).unwrap();
    assert_eq!(uart.interrupt(), Some(InterruptLine::MachineExternal));
    assert_eq!(uart.read(0, 4).unwrap(), 65);
    assert_eq!(uart.interrupt(), None);
    uart.write(0, 4, 66).unwrap();
    assert_eq!(uart.transmitted(), b"B");
}

#[test]
fn transmitter_readiness_tracks_capture_capacity_for_both_layouts() {
    for (layout, status, ready) in [
        (UartLayout::Simple, 4, 2),
        (UartLayout::Ns16550, 0x14, 0x20),
    ] {
        let mut uart = Uart::with_layout(layout);
        uart.inject_receive(b"R").unwrap();
        assert_ne!(uart.read(status, 4).unwrap() & ready, 0);
        for _ in 0..65536 {
            uart.write(0, 4, 65).unwrap();
        }
        assert_eq!(uart.read(status, 4).unwrap() & ready, 0, "{layout:?}");
        assert_eq!(uart.read(status, 4).unwrap() & 1, 1);
        assert!(uart.write(0, 4, 66).is_err());
        assert_eq!(uart.transmitted().len(), 65536);
        let mut output = Vec::new();
        uart.drain_transmitted(&mut output);
        assert_eq!(output, vec![65; 65536]);
        assert_ne!(uart.read(status, 4).unwrap() & ready, 0);
        uart.write(0, 4, 66).unwrap();
        assert_eq!(uart.transmitted(), b"B");
        assert_eq!(uart.read(0, 4).unwrap(), u32::from(b'R'));
    }
}
