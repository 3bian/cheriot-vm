// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::InterruptLine;

pub struct DeviceActivity {
    pub bus_master: bool,
    pub edge_triggered: bool,
    pub host_errors: bool,
    pub interrupt_lines: &'static [InterruptLine],
}

impl Default for DeviceActivity {
    fn default() -> Self {
        Self {
            bus_master: false,
            edge_triggered: false,
            host_errors: true,
            interrupt_lines: &[
                InterruptLine::MachineExternal,
                InterruptLine::MachineSoftware,
                InterruptLine::MachineTimer,
            ],
        }
    }
}
