// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub mod connections;
#[cfg(all(feature = "gdb", unix))]
pub mod debugger;
mod device_path;
pub mod device_selection;
pub mod ethernet;
pub mod gpio;
pub mod i2c;
pub mod open_flags;
pub mod spi;
pub mod uart;

pub use connections::HostConnections;
pub use i2c::connection::I2cConnection;
pub use spi::connection::SpiConnection;
