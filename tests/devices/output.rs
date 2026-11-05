// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::bus::Bus;
use cheriot_vm::devices::uart::{Uart, UartLayout};
use cheriot_vm::devices::InterruptLine;

#[test]
fn output_draining_preserves_device_order_input_and_capture_limits() {
    let mut bus = Bus::new();
    bus.add_device(
        "console".into(),
        0x1000,
        256,
        Box::new(Uart::with_layout(UartLayout::Ns16550)),
    )
    .unwrap();
    let mut uart = Uart::new();
    uart.inject_receive(b"input").unwrap();
    bus.add_device("uart".into(), 0x2000, 12, Box::new(uart))
        .unwrap();
    bus.write(0x2008, 4, 1).unwrap();

    for _ in 0..65536 {
        bus.write(0x1000, 4, 0xff).unwrap();
        bus.write(0x2000, 4, b'U'.into()).unwrap();
    }

    assert!(bus.write(0x1000, 4, 0).is_err());
    assert!(bus.write(0x2000, 4, 0).is_err());
    assert_eq!(bus.read(0x2004, 4).unwrap(), 1);
    assert!(bus.interrupt_pending(InterruptLine::MachineExternal));
    bus.check_host_errors().unwrap();

    let mut output = vec![0];
    bus.drain_output(&mut output);
    assert_eq!(output.len(), 131073);
    assert_eq!(output[0], 0);
    assert!(output[1..65537].iter().all(|byte| *byte == 0xff));
    assert!(output[65537..].iter().all(|byte| *byte == b'U'));
    assert_eq!(bus.read(0x2004, 4).unwrap(), 3);
    assert!(bus.interrupt_pending(InterruptLine::MachineExternal));
    assert_eq!(bus.read(0x2000, 4).unwrap(), u32::from(b'i'));

    bus.drain_output(&mut output);
    assert_eq!(output.len(), 131073);
    bus.write(0x1000, 4, b'C'.into()).unwrap();
    bus.write(0x2000, 4, b'U'.into()).unwrap();
    output.clear();
    bus.drain_output(&mut output);
    assert_eq!(output, b"CU");
}
