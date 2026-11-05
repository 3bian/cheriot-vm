// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;
use crate::cli::{gpio_options::GpioOptions, tap_options::TapOptions};

fn prepare(
    bus: &Bus,
    gpio: Vec<GpioOptions>,
    tap: Vec<TapOptions>,
) -> Result<Vec<HostConnections>, EmulatorError> {
    super::prepare(
        bus,
        HostOptions {
            gpio,
            tap,
            ..HostOptions::default()
        },
    )
}
use cheriot_vm::devices::ethernet::NetworkBackend;

fn bus(count: u32) -> Bus {
    let mut bus = Bus::new();
    for index in 0..count {
        let base = 0x10000000 + index * 0x10000;
        bus.add_device(format!("gpio{index}"), base, 0x1000, Box::new(Gpio::new()))
            .unwrap();
        bus.add_device(
            format!("ethernet{index}"),
            base + 0x1000,
            0x1000,
            Box::new(Ethernet::new(NetworkBackend::InMemory)),
        )
        .unwrap();
    }
    for index in 0..count {
        let base = 0x20000000 + index * 0x10000;
        bus.add_device(format!("spi{index}"), base, 256, Box::new(Spi::new()))
            .unwrap();
        bus.add_device(
            format!("i2c{index}"),
            base + 0x1000,
            256,
            Box::new(I2c::new()),
        )
        .unwrap();
    }
    bus
}

#[test]
fn automatic_device_selection_requires_exactly_one_matching_device() {
    for count in [0, 2] {
        let error = prepare(
            &bus(count),
            vec![GpioOptions {
                settings: Default::default(),
                device: None,
                lines: vec![17, 27],
                path: "/dev/gpiochip0".into(),
            }],
            vec![],
        )
        .err()
        .unwrap()
        .to_string();
        assert!(
            error.contains(if count == 0 {
                "no GPIO"
            } else {
                "gpio0, gpio1"
            }),
            "{error}"
        );
    }
    let connections = prepare(
        &bus(1),
        vec![GpioOptions {
            settings: Default::default(),
            device: None,
            lines: vec![17, 27],
            path: "/dev/gpiochip0".into(),
        }],
        vec![TapOptions {
            device: None,
            interface: "tap0".into(),
        }],
    )
    .unwrap();
    assert_eq!(connections[0].gpio_chip.as_ref().unwrap().0, "gpio0");
    assert_eq!(
        connections[1].tap_interface.as_ref().unwrap().0,
        "ethernet0"
    );
    let error = prepare(
        &bus(2),
        vec![],
        vec![TapOptions {
            device: None,
            interface: "tap0".into(),
        }],
    )
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("ethernet0, ethernet1"), "{error}");
}

#[test]
fn explicit_multi_device_bindings_are_matched_by_name_in_any_order() {
    let gpio = [1, 0].map(|index| GpioOptions {
        settings: Default::default(),
        device: Some(format!("gpio{index}")),
        lines: vec![index * 2, index * 2 + 1],
        path: "/dev/gpiochip0".into(),
    });
    let tap = [0, 1].map(|index| TapOptions {
        device: Some(format!("ethernet{index}")),
        interface: format!("tap{index}"),
    });
    let connections = prepare(&bus(2), gpio.into(), tap.into()).unwrap();
    assert_eq!(connections.len(), 4);
    assert_eq!(connections[0].gpio_chip.as_ref().unwrap().0, "gpio1");
    assert_eq!(connections[0].gpio_lines.as_ref().unwrap(), &[2, 3]);
    assert_eq!(connections[1].gpio_chip.as_ref().unwrap().0, "gpio0");
    assert_eq!(connections[2].tap_interface.as_ref().unwrap().1, "tap0");
    assert_eq!(connections[3].tap_interface.as_ref().unwrap().1, "tap1");
}

#[test]
fn duplicate_devices_and_overlapping_host_resources_fail_before_attachment() {
    for (first, second, interface) in [
        ("ethernet0", "ethernet0", "tap1"),
        ("ethernet0", "ethernet1", "tap0"),
    ] {
        let error = prepare(
            &bus(2),
            vec![],
            vec![
                TapOptions {
                    device: Some(first.into()),
                    interface: "tap0".into(),
                },
                TapOptions {
                    device: Some(second.into()),
                    interface: interface.into(),
                },
            ],
        )
        .err()
        .unwrap()
        .to_string();
        assert!(
            error.contains(if first == second {
                "duplicate TAP binding"
            } else {
                "only be bound once"
            }),
            "{error}"
        );
    }
    let error = prepare(
        &bus(2),
        [0, 1]
            .map(|index| GpioOptions {
                settings: Default::default(),
                device: Some(format!("gpio{index}")),
                lines: vec![17],
                path: "/dev/gpiochip0".into(),
            })
            .into(),
        vec![],
    )
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("unique across bindings"), "{error}");
}

#[cfg(target_os = "linux")]
#[test]
fn spi_and_i2c_selection_use_the_same_single_device_rule_and_allow_multiple_controllers() {
    use crate::cli::{i2c_options::I2cOptions, spi_options::SpiOptions};
    for count in [0, 2] {
        for spi in [false, true] {
            let mut options = HostOptions::default();
            if spi {
                options.spi.push(SpiOptions {
                    device: None,
                    path: "/dev/null".into(),
                    select: 0,
                    speed: 500_000,
                });
            } else {
                options.i2c.push(I2cOptions {
                    addresses: None,
                    device: None,
                    path: "/dev/zero".into(),
                });
            }
            let error = super::prepare(&bus(count), options)
                .err()
                .unwrap()
                .to_string();
            assert!(
                error.contains(if count == 0 {
                    "no "
                } else if spi {
                    "spi0, spi1"
                } else {
                    "i2c0, i2c1"
                }),
                "{error}"
            );
        }
    }
    for count in [1, 2] {
        let options = HostOptions {
            spi: vec![SpiOptions {
                device: (count == 2).then(|| "spi1".into()),
                path: "/dev/null".into(),
                select: 2,
                speed: 1_000_000,
            }],
            i2c: vec![I2cOptions {
                addresses: None,
                device: (count == 2).then(|| "i2c1".into()),
                path: "/dev/zero".into(),
            }],
            ..HostOptions::default()
        };
        let connections = super::prepare(&bus(count), options).unwrap();
        assert_eq!(
            connections[0].spi.as_ref().unwrap().device,
            format!("spi{}", count - 1)
        );
        assert_eq!(
            connections[1].i2c.as_ref().unwrap().device,
            format!("i2c{}", count - 1)
        );
    }
}
