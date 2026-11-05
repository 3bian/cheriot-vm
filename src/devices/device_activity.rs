// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::InterruptLine;

pub struct DeviceActivity {
    pub host_errors: bool,
    pub interrupt_lines: &'static [InterruptLine],
}

impl Default for DeviceActivity {
    fn default() -> Self {
        Self {
            host_errors: true,
            interrupt_lines: &[
                InterruptLine::MachineExternal,
                InterruptLine::MachineSoftware,
                InterruptLine::MachineTimer,
            ],
        }
    }
}
