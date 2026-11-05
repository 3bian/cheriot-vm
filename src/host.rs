// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub mod connections;
#[cfg(all(feature = "gdb", target_os = "linux"))]
pub mod debugger;
mod device_path;
pub mod device_selection;
pub mod ethernet;
pub mod gpio;
pub mod i2c;
pub mod spi;
pub mod uart;

pub use connections::HostConnections;
pub use i2c::connection::I2cConnection;
pub use spi::connection::SpiConnection;
