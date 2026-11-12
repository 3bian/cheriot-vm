// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

fn request() -> MachineRequest {
    MachineRequest::new(
        Firmware::Raw {
            path: "firmware".into(),
            address: 0,
        },
        "profile".into(),
    )
}

#[test]
fn checked_addresses_preserve_formats_and_specific_errors() {
    for (text, expected) in [
        ("0", 0),
        ("+42", 42),
        ("0x2a", 42),
        ("0xffffffff", u32::MAX),
    ] {
        assert_eq!(address(text, AddressKind::Raw).unwrap(), expected);
    }
    for text in ["-1", " 1", "1 ", "0X10", "0x", "18446744073709551616"] {
        assert_eq!(
            address(text, AddressKind::Raw).unwrap_err().to_string(),
            format!("configuration error: invalid number: {text}")
        );
    }
    for (kind, message) in [
        (AddressKind::Raw, "raw"),
        (AddressKind::Breakpoint, "breakpoint"),
        (AddressKind::Completion, "completion"),
    ] {
        assert_eq!(
            address("0x100000000", kind).unwrap_err().to_string(),
            format!("configuration error: {message} address exceeds 32 bits")
        );
    }
}

#[test]
fn connections_validate_settings_and_preserve_multiple_devices() {
    let mut req = request();
    req.peripherals = PeripheralSettings {
        uart: vec![
            "out=/dev/stdout,in=/dev/stdin,device=uart1".into(),
            "device=uart0,out=serial.log".into(),
        ],
        tap: vec![
            "interface=tap1,device=ethernet1".into(),
            "device=ethernet0,interface=tap0".into(),
        ],
        gpio: vec!["lines=0x11+27,device=gpio1,path=/dev/gpiochip0".into()],
        spi: vec![
            "path=/dev/spidev0.0,select=2,speed=1000000".into(),
            "device=spi1,path=/dev/spidev1.0".into(),
        ],
        i2c: vec![
            "device=i2c0,path=/dev/i2c-1".into(),
            "device=i2c1,path=/dev/i2c-2".into(),
        ],
    };
    let options = req.validate().unwrap();
    assert_eq!(options.host.uart.len(), 2);
    assert_eq!(options.host.uart[0].device.as_deref(), Some("uart1"));
    assert_eq!(options.host.uart[1].output.as_deref(), Some("serial.log"));
    assert_eq!(options.host.gpio[0].lines, [17, 27]);
    assert_eq!(options.host.tap.len(), 2);
    assert_eq!(options.host.spi[0].select, 2);
    assert_eq!(options.host.spi[0].speed, 1_000_000);
    assert_eq!(options.host.spi[1].speed, 500_000);
    assert_eq!(options.host.i2c.len(), 2);
}

#[test]
fn invalid_connections_and_excess_bindings_are_rejected() {
    for (kind, value) in [
        ("spi", "path=/dev/spidev0.0,select=4"),
        ("spi", "path=/dev/spidev0.0,speed=0"),
        ("spi", "path=/dev/spidev0.0,speed=50000001"),
        ("i2c", "path=/dev/i2c-1,address=0x50"),
        ("gpio", "path=/dev/gpiochip0,lines=1,bias=unknown"),
        ("gpio", "path=/dev/gpiochip0,lines=1,drive=unknown"),
        ("gpio", "path=/dev/gpiochip0,lines=1,active-low=yes"),
        ("i2c", "path=/dev/i2c-1,addresses=0x07"),
        ("i2c", "path=/dev/i2c-1,addresses=0x50+0x50"),
        ("uart", "in=/dev/stdin,baud=12345"),
        ("uart", "in=/dev/stdin,data-bits=9"),
        ("uart", "in=/dev/stdin,stop-bits=3"),
        ("uart", "in=/dev/stdin,parity=mark"),
        ("uart", "in=/dev/stdin,flow=unknown"),
        ("uart", ""),
        ("uart", "device=uart0"),
        ("uart", "in="),
        ("uart", "in=/dev/stdin,in=/tmp/input"),
        ("uart", "input=/dev/stdin"),
        ("tap", "interface=tap0,"),
        ("tap", "device=ethernet0"),
        ("gpio", "path=/dev/gpiochip0,lines=1,2"),
        ("gpio", "path=/dev/gpiochip0,lines="),
        ("gpio", "path=/dev/gpiochip0,lines=4294967296"),
        ("uart", "in=/dev/stdin,baud=4294967296"),
    ] {
        let mut req = request();
        let values = match kind {
            "spi" => &mut req.peripherals.spi,
            "i2c" => &mut req.peripherals.i2c,
            "gpio" => &mut req.peripherals.gpio,
            "uart" => &mut req.peripherals.uart,
            "tap" => &mut req.peripherals.tap,
            _ => unreachable!(),
        };
        values.push(value.into());
        assert!(req.validate().is_err(), "{kind} {value}");
    }
    let mut req = request();
    req.peripherals.uart = (0..65)
        .map(|i| format!("device=uart{i},out=/dev/stdout"))
        .collect();
    assert!(
        req.validate()
            .err()
            .unwrap()
            .to_string()
            .contains("64 host bindings")
    );
}

#[test]
fn memory_and_register_selections_are_validated() {
    for (value, expected) in [
        ("address=0x80000000", (0x80000000, 16)),
        ("length=256,address=0xffffff00", (0xffffff00, 256)),
        ("address=4294967295,length=1", (u32::MAX, 1)),
    ] {
        let mut req = request();
        req.memory = Some(value.into());
        assert_eq!(req.validate().unwrap().memory_dump, Some(expected));
    }
    for value in [
        "0x80000000:32",
        "length=32",
        "address=0,length=0",
        "address=0,length=257",
        "address=0xffffffff,length=2",
        "address=0,address=1",
        "address=4294967296",
    ] {
        let mut req = request();
        req.memory = Some(value.into());
        assert!(req.validate().is_err(), "{value}");
    }
    for name in ["x15", "c1", "pcc"] {
        let mut req = request();
        req.registers = Some(RegisterDump::Selection(format!("names={name}")));
        assert!(req.validate().is_ok(), "{name}");
    }
    for name in ["x32", "c16", "unknown", "x1+x01", "x1+", "+x1"] {
        let mut req = request();
        req.registers = Some(RegisterDump::Selection(format!("names={name}")));
        assert!(req.validate().is_err(), "{name}");
    }
}

#[test]
fn host_settings_retain_explicit_configuration() {
    use crate::exec::machine::host::{
        gpio::{bias::GpioBias, drive::GpioDrive},
        uart::{flow::UartFlow, parity::UartParity},
    };
    let mut req = request();
    req.peripherals.gpio.push(
        "path=/dev/gpiochip0,lines=17+27,active-low=true,bias=pull-up,drive=open-drain".into(),
    );
    req.peripherals
        .i2c
        .push("path=/dev/i2c-1,addresses=0x50+0x68".into());
    req.peripherals
        .uart
        .push("in=/dev/ttyUSB0,baud=115200,data-bits=7,parity=even,stop-bits=2,flow=rtscts".into());
    let options = req.validate().unwrap();
    let gpio = options.host.gpio[0].settings;
    assert!(gpio.active_low);
    assert_eq!(gpio.bias, Some(GpioBias::PullUp));
    assert_eq!(gpio.drive, GpioDrive::OpenDrain);
    assert_eq!(
        options.host.i2c[0].addresses.as_deref(),
        Some([0x50, 0x68].as_slice())
    );
    let uart = options.host.uart[0].settings.unwrap();
    assert_eq!(
        (
            uart.baud,
            uart.data_bits,
            uart.parity,
            uart.stop_bits,
            uart.flow
        ),
        (Some(115200), 7, UartParity::Even, 2, UartFlow::RtsCts)
    );
}

#[test]
fn debugger_rejects_non_loopback_and_conflicting_options() {
    let mut req = request();
    req.debugger = Some("192.0.2.1:3333".parse().unwrap());
    assert!(
        req.validate()
            .err()
            .unwrap()
            .to_string()
            .contains("loopback")
    );
    if cfg!(all(feature = "gdb", unix)) {
        let mut raw = request();
        raw.debugger = Some("127.0.0.1:3333".parse().unwrap());
        raw.debugger_raw_output = true;
        assert!(raw.validate().unwrap().gdb_raw_output);
        let mut req = request();
        req.debugger = Some("127.0.0.1:3333".parse().unwrap());
        req.trace = true;
        assert!(
            req.validate()
                .err()
                .unwrap()
                .to_string()
                .contains("debug does not take")
        );
    }
}
