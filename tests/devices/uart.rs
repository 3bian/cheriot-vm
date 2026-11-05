// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::devices::uart::Uart;
use cheriot_vm::devices::Device;

#[test]
fn uart_buffers_are_bounded_and_can_be_drained() {
    let mut uart = Uart::new();
    uart.inject_receive(&vec![42; 4096]).unwrap();
    assert!(uart.inject_receive(b"x").is_err());
    assert_eq!(uart.read(0, 4).unwrap(), 42);
    uart.inject_receive(b"x").unwrap();

    for _ in 0..65536 {
        uart.write(0, 4, 65).unwrap();
    }

    assert_eq!(uart.read(4, 4).unwrap() & 2, 0);
    assert!(uart.write(0, 4, 66).is_err());
    assert_eq!(uart.take_transmitted().len(), 65536);
    uart.write(0, 4, 66).unwrap();
    assert_eq!(uart.transmitted(), b"B");
}
