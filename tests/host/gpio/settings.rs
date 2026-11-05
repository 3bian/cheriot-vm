// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;
use gpiocdev::line::{Bias, Direction, Drive, Value};

#[test]
fn polarity_bias_and_drive_are_preserved_when_directions_change() {
    let settings = GpioSettings {
        active_low: true,
        bias: Some(GpioBias::PullUp),
        drive: GpioDrive::OpenDrain,
    };
    let mut config = gpiocdev::request::Config::default();
    config.with_lines(&[17, 27]).as_input();
    settings.configure(&mut config);
    for offset in [17, 27] {
        let line = config.line_config(offset).unwrap();
        assert_eq!(line.direction, Some(Direction::Input));
        assert!(line.active_low);
        assert_eq!(line.bias, Some(Bias::PullUp));
        assert_eq!(line.drive, None);
    }
    config
        .with_line(17)
        .as_output(Value::Active)
        .with_drive(settings.drive());
    let line = config.line_config(17).unwrap();
    assert_eq!(line.drive, Some(Drive::OpenDrain));
    assert_eq!(line.bias, Some(Bias::PullUp));
    assert!(line.active_low);
    config.as_input();
    assert_eq!(config.line_config(17).unwrap().drive, None);
}
