// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{bias::GpioBias, drive::GpioDrive};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpioSettings {
    pub active_low: bool,
    pub bias: Option<GpioBias>,
    pub drive: GpioDrive,
}

impl GpioSettings {
    #[cfg(target_os = "linux")]
    pub(super) fn configure(&self, configuration: &mut gpiocdev::request::Config) {
        use gpiocdev::line::Bias;
        if self.active_low {
            configuration.as_active_low();
        } else {
            configuration.as_active_high();
        }
        configuration.with_bias(self.bias.map(|bias| match bias {
            GpioBias::Disabled => Bias::Disabled,
            GpioBias::PullDown => Bias::PullDown,
            GpioBias::PullUp => Bias::PullUp,
        }));
    }

    #[cfg(target_os = "linux")]
    pub(super) fn drive(&self) -> gpiocdev::line::Drive {
        match self.drive {
            GpioDrive::OpenDrain => gpiocdev::line::Drive::OpenDrain,
            GpioDrive::OpenSource => gpiocdev::line::Drive::OpenSource,
            GpioDrive::PushPull => gpiocdev::line::Drive::PushPull,
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../../tests/host/gpio/settings.rs"]
mod tests;
