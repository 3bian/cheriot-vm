// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::DeviceMapping;
use crate::devices::interrupt_controller::InterruptController;
use crate::devices::InterruptLine;

#[derive(Default)]
pub(super) struct DeviceIndices {
    pub external_interrupts: Vec<usize>,
    pub host_errors: Vec<usize>,
    pub interrupt_controllers: Vec<usize>,
    pub routed_interrupts: Vec<(usize, u8)>,
    pub software_interrupts: Vec<usize>,
    pub timer_interrupts: Vec<usize>,
}

impl DeviceIndices {
    pub fn new(devices: &[DeviceMapping]) -> Self {
        let mut indices = Self::default();

        for (index, mapping) in devices.iter().enumerate() {
            let activity = mapping.device.activity();

            if activity.host_errors {
                indices.host_errors.push(index);
            }

            if mapping.device.as_any().is::<InterruptController>() {
                indices.interrupt_controllers.push(index);
            }

            if let Some(source) = mapping.interrupt_source {
                if (1..32).contains(&source) {
                    indices.routed_interrupts.push((index, source));
                }

                continue;
            }

            for line in activity.interrupt_lines {
                match line {
                    InterruptLine::MachineExternal => indices.external_interrupts.push(index),
                    InterruptLine::MachineSoftware => indices.software_interrupts.push(index),
                    InterruptLine::MachineTimer => indices.timer_interrupts.push(index),
                }
            }
        }

        indices
    }
}
