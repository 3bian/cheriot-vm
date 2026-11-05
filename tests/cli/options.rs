// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn singleton_values_reject_duplicates_and_boolean_switches_are_idempotent() {
    for (flag, value) in [
        ("--profile", "profile"),
        ("--address", "0"),
        ("--steps", "0"),
        ("--breakpoint", "0"),
        ("--memory", "address=0,length=1"),
        ("--registers", "names=x1"),
        ("--gdb", "port=333"),
        ("--trap-policy", "stop"),
        ("--completion", "disabled"),
        ("--output", "raw"),
        ("--checkpoint", "snapshot"),
    ] {
        let mut arguments: Vec<_> = ["--raw", "firmware"]
            .into_iter()
            .map(String::from)
            .collect();
        arguments.extend([flag, value, flag, value].into_iter().map(String::from));
        assert!(
            Options::parse(&arguments)
                .err()
                .unwrap()
                .to_string()
                .contains("once"),
            "{flag}"
        );
    }
    let arguments = [
        "--profile",
        "profile",
        "--raw",
        "firmware",
        "--trace",
        "--trace",
        "--registers",
    ]
    .map(String::from);
    assert!(Options::parse(&arguments).is_ok());
}

fn arguments(options: &[&str]) -> Vec<String> {
    ["--profile", "profile", "--raw", "firmware"]
        .into_iter()
        .chain(options.iter().copied())
        .map(String::from)
        .collect()
}

#[test]
fn connection_settings_allow_reordering_and_repeated_flags_for_different_devices() {
    let options = Options::parse(&arguments(&[
        "--uart",
        "out=/dev/stdout,in=/dev/stdin,device=uart1",
        "--tap",
        "interface=tap1,device=ethernet1",
        "--gpio",
        "lines=0x11+27,device=gpio1,path=/dev/gpiochip0",
        "--uart",
        "device=uart0,out=serial.log",
        "--spi",
        "path=/dev/spidev0.0,select=2,speed=1000000",
        "--spi",
        "device=spi1,path=/dev/spidev1.0",
        "--i2c",
        "device=i2c0,path=/dev/i2c-1",
        "--i2c",
        "device=i2c1,path=/dev/i2c-2",
        "--tap",
        "device=ethernet0,interface=tap0",
    ]))
    .unwrap();
    assert_eq!(options.host.uart.len(), 2);
    assert_eq!(options.host.uart[0].device.as_deref(), Some("uart1"));
    assert_eq!(options.host.uart[0].input.as_deref(), Some("/dev/stdin"));
    assert_eq!(options.host.uart[1].output.as_deref(), Some("serial.log"));
    assert_eq!(options.host.gpio[0].lines, [17, 27]);
    assert_eq!(options.host.tap.len(), 2);
    assert_eq!(options.host.spi.len(), 2);
    assert_eq!(options.host.spi[0].select, 2);
    assert_eq!(options.host.spi[0].speed, 1_000_000);
    assert_eq!(options.host.spi[1].speed, 500_000);
    assert_eq!(options.host.i2c.len(), 2);
}

#[test]
fn connection_settings_reject_missing_empty_unknown_and_duplicate_values() {
    for (flag, value) in [
        ("--spi", "path=/dev/spidev0.0,select=4"),
        ("--spi", "path=/dev/spidev0.0,speed=0"),
        ("--spi", "path=/dev/spidev0.0,speed=50000001"),
        ("--i2c", "path=/dev/i2c-1,address=0x50"),
        ("--gpio", "path=/dev/gpiochip0,lines=1,bias=unknown"),
        ("--gpio", "path=/dev/gpiochip0,lines=1,drive=unknown"),
        ("--gpio", "path=/dev/gpiochip0,lines=1,active-low=yes"),
        ("--i2c", "path=/dev/i2c-1,addresses=0x07"),
        ("--i2c", "path=/dev/i2c-1,addresses=0x50+0x50"),
        ("--uart", "in=/dev/stdin,baud=12345"),
        ("--uart", "in=/dev/stdin,data-bits=9"),
        ("--uart", "in=/dev/stdin,stop-bits=3"),
        ("--uart", "in=/dev/stdin,parity=mark"),
        ("--uart", "in=/dev/stdin,flow=unknown"),
        ("--uart", ""),
        ("--uart", "device=uart0"),
        ("--uart", "in="),
        ("--uart", "in=/dev/stdin,in=/tmp/input"),
        ("--uart", "input=/dev/stdin"),
        ("--tap", "interface=tap0,"),
        ("--tap", "device=ethernet0"),
        ("--gpio", "path=/dev/gpiochip0,lines=1,2"),
        ("--gpio", "path=/dev/gpiochip0,lines="),
    ] {
        assert!(
            Options::parse(&arguments(&[flag, value])).is_err(),
            "{flag} {value}"
        );
    }
    let mut many = arguments(&[]);
    for index in 0..65 {
        many.extend([
            "--uart".into(),
            format!("device=uart{index},out=/dev/stdout"),
        ]);
    }
    assert!(Options::parse(&many)
        .err()
        .unwrap()
        .to_string()
        .contains("64 host bindings"));
}

#[cfg(all(feature = "gdb", target_os = "linux"))]
#[test]
fn gdb_defaults_and_individual_settings_do_not_consume_following_flags() {
    for (flags, expected) in [
        (vec!["--gdb"], "127.0.0.1:333"),
        (vec!["--gdb", "--steps", "10"], "127.0.0.1:333"),
        (vec!["--gdb", "port=3333,output=raw"], "127.0.0.1:3333"),
        (vec!["--gdb", "ip=::1"], "[::1]:333"),
        (vec!["--gdb", "port=0,ip=127.0.0.2"], "127.0.0.2:0"),
    ] {
        let options = Options::parse(&arguments(&flags)).unwrap();
        assert_eq!(options.gdb.unwrap().to_string(), expected);
    }
}

#[test]
fn obsolete_long_flags_are_rejected() {
    for flag in [
        "--gdb-raw-output",
        "--uart-input",
        "--uart-in",
        "--uart-out",
        "--uart-device",
        "--tap-device",
        "--tap-interface",
        "--gpio-device",
        "--gpio-path",
        "--gpio-lines",
        "--gdb-ip",
        "--gdb-port",
    ] {
        let error = Options::parse(&arguments(&[flag, "unused"]))
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.contains(&format!("unknown argument: {flag}")),
            "{error}"
        );
    }
}

#[test]
fn memory_settings_validate_lengths_defaults_duplicates_and_address_overflow() {
    for (value, expected) in [
        ("address=0x80000000", (0x80000000, 16)),
        ("length=256,address=0xffffff00", (0xffffff00, 256)),
        ("address=4294967295,length=1", (u32::MAX, 1)),
    ] {
        let options = Options::parse(&arguments(&["--memory", value])).unwrap();
        assert_eq!(options.memory_dump, Some(expected));
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
        assert!(
            Options::parse(&arguments(&["--memory", value])).is_err(),
            "{value}"
        );
    }
}

#[test]
fn register_settings_default_to_all_and_reject_unknown_names() {
    use crate::cli::register::Register;
    for (values, expected) in [
        (vec!["--registers"], RegisterSelection::All),
        (
            vec!["--registers", "names=x15"],
            RegisterSelection::Names(vec![Register::Integer(15)]),
        ),
        (
            vec!["--registers", "names=c1"],
            RegisterSelection::Names(vec![Register::Capability(1)]),
        ),
        (
            vec!["--registers", "names=pcc"],
            RegisterSelection::Names(vec![Register::Pcc]),
        ),
    ] {
        assert_eq!(
            Options::parse(&arguments(&values)).unwrap().registers,
            Some(expected)
        );
    }
    for name in ["x32", "c16", "unknown", "x1+x01", "x1+", "+x1"] {
        assert!(Options::parse(&arguments(&["--registers", &format!("names={name}")])).is_err());
    }
}

#[test]
fn optional_host_settings_parse_without_changing_simulated_device_defaults() {
    use cheriot_vm::host::gpio::bias::GpioBias;
    use cheriot_vm::host::gpio::drive::GpioDrive;
    use cheriot_vm::host::uart::flow::UartFlow;
    use cheriot_vm::host::uart::parity::UartParity;
    let options = Options::parse(&arguments(&[
        "--gpio",
        "path=/dev/gpiochip0,lines=17+27,active-low=true,bias=pull-up,drive=open-drain",
        "--i2c",
        "path=/dev/i2c-1,addresses=0x50+0x68",
        "--uart",
        "in=/dev/ttyUSB0,baud=115200,data-bits=7,parity=even,stop-bits=2,flow=rtscts",
    ]))
    .unwrap();
    let gpio = options.host.gpio[0].settings;
    assert!(gpio.active_low);
    assert_eq!(gpio.bias, Some(GpioBias::PullUp));
    assert_eq!(gpio.drive, GpioDrive::OpenDrain);
    assert_eq!(
        options.host.i2c[0].addresses.as_deref(),
        Some([0x50, 0x68].as_slice())
    );
    let uart = options.host.uart[0].settings.unwrap();
    assert_eq!(uart.baud, Some(115200));
    assert_eq!(uart.data_bits, 7);
    assert_eq!(uart.parity, UartParity::Even);
    assert_eq!(uart.stop_bits, 2);
    assert_eq!(uart.flow, UartFlow::RtsCts);
}
